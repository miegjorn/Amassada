# Amassada — Design Spec
**Date:** 2026-06-12
**Status:** Approved for implementation

---

## 1. Purpose

Amassada is the multi-agent session engine of the Occitan stack.

### Foundational axiom

**An agent is the callable interface to a dynamically assembled context. Context is produced
by agents interacting. Therefore agents are made of context, and context is made by agents —
the system is self-referential by design.**

A session transcript is context. That context can be addressed as an agent in a future session.
That agent's contributions enrich the context further. There is no fixed ground floor.

The address `project-infra+adversarial` is a live pointer — it resolves differently each time
it is dereferenced, depending on what Farga currently holds for `project-infra`. As the project
accumulates sessions and artifacts, the same address surfaces a richer agent. Every session
enriches the substrate from which future agents are grown.

This is the self-resolving loop: each session rotates the cube, and the cube's new state
shapes what the next rotation can do.

### What Amassada does

A **session** is a structured conversation between dynamically composed agents, assembled to
address a challenge. Sessions are governed by a **Moderator** — a role any agent can hold —
and structured into **rounds** and **turns**, following rules declared in a **canvas** (YAML).
The Moderator is a real AI agent that can reshape the session mid-flight.

Amassada runs identically in two execution contexts:
- **Local / CLI** — no chatroom, auto or interactive, stdout/buffer I/O
- **Charradissa** — live Matrix rooms, agents as room members, real-time streaming

The same session logic applies in both; only the transport layer differs.

---

## 2. Architecture

Amassada is a **Cargo workspace** with two crates:

### `amassada-core` (library)
Pure session logic. No I/O. Contains the state machine, canvas parser, budget accounting,
block parser, channel runtime, and transport trait.

### `amassada-server` (binary)
Thin Axum service wrapping the core for remote use:
- REST endpoints (start session, query state, post human input)
- WebSocket endpoint for real-time event streaming

### Key seam: `Transport` trait
Abstracts execution context. Two implementations ship:
- `LocalTransport` — stdout/channel buffer, `recv_human` reads stdin or injected async channel
- `CharradissaTransport` — Matrix room operations (feature-gated: `--features charradissa`)

---

## 3. Channel Architecture

Sessions run across **three async channels**. The DEBATE channel is sequential (ordered transcript);
the other two run concurrently around it.

    ┌──────────────────────────────────────────────────────────────────┐
    │                                                                  │
    │  MAIN SESSION  (broadcast::channel)                             │
    │  ─────────────────────────────────                              │
    │  The transcript. All [MAIN] and [BTW] blocks appended here.     │
    │  Every agent subscribes. build_context() reads the last N msgs. │
    │  80 % of total token budget.                                    │
    │                                                                  │
    │  CONSULT  (mpsc::channel per pair, concurrent)                  │
    │  ─────────────────────────────────────────────                  │
    │  Private [CONSULT] sidebars. Invisible to transcript.           │
    │  Multiple [CONSULT] blocks dispatched with tokio::join_all().   │
    │  Result injected into requester context only.                   │
    │  15 % of total token budget.                                    │
    │                                                                  │
    │  MOD WHISPER  (HashMap<AgentId, mpsc::Sender>, per-agent)       │
    │  ─────────────────────────────────────────────────────          │
    │  Moderator → specific agent, private, non-blocking.             │
    │  Steering: "be concise", "budget at 15%", "stay on topic".      │
    │  Injected into agent context before their next turn.            │
    │  5 % of total token budget.                                     │
    │                                                                  │
    └──────────────────────────────────────────────────────────────────┘

    Note: MAIN SESSION and DEBATE are two layers of the same budget pool.
    MAIN SESSION = visibility layer (broadcast, agents read).
    DEBATE       = dispatch layer  (mpsc, turn engine writes).
    Tokens are charged once when a [MAIN] block is committed.

---

## 4. Canvas Format

A canvas is a YAML file that defines **sensible defaults** for a session. All numeric and
structural fields are defaults the Moderator can override at runtime via moderator blocks.
The only hard constraints enforced by the engine are total token budget (can't spend what
doesn't exist) and human authority (Moderator cannot override a confirmed `/call`).

Canvases live in `canvases/stdlib/` and are matched automatically from an intake prompt
by `CanvasSelector`.

### Example canvas

    id: aws-architecture-review
    version: "1.0.0"
    mode: interactive          # or: auto

    selector:
      description: "Structured multi-agent debate with opposing positions"
      tags: [debate, argument, positions, pros-cons, decision]
      examples:
        - "should we use microservices or a monolith?"
        - "debate the tradeoffs of event sourcing"

    initial_participants:
      # Composition address: domain × facet × persona — resolved at dispatch by Fondament/Farga
      # persona: the cognitive stance (builder, breaker, realist, adversarial, dreamer, moderator)
      # domain: the context body to draw from (Fondament primitive or Farga project/component)
      # facet:  optional narrowing of the domain (e.g. project-infra, project-auth)
      # model:  optional L2 override — Moderator can still override at L3 mid-session
      - persona: moderator           # special: grants moderator block set
        domain: fondament/tech-moderator
      - persona: builder
        domain: fondament/aws-architect
      - persona: adversarial
        domain: project/infra        # Farga project facet as agent domain
        model: claude-sonnet-4-6    # L2 model override
      - persona: human
        authority: binding

    budget:
      total_tokens: 100_000    # HARD LIMIT — engine enforced, never exceedable
      pools:
        main_session:  80_000  # default 80% — Moderator can [ADJUST_BUDGET]
        consultations: 15_000  # default 15% — Moderator can [ADJUST_BUDGET]
        mod_whisper:    5_000  # default  5% — Moderator can [ADJUST_BUDGET]

    consultation:
      max_turns: 2             # default — Moderator can ignore
      min_response_tokens: 50  # default — Moderator can ignore

    rounds:
      min: 2                   # default — Moderator can [CLOSE] earlier
      max: 5                   # default — Moderator can run longer
      convergence_modifier: 0.8  # default — Moderator can ignore
      context_window: 20       # default — Moderator can request broader context

    human:
      slot: true
      advisory_window_turns: 1 # default — Moderator can extend

    output:
      format: markdown
      sections:
        - id: decision
          title: "Architecture Decision"
          required: true
        - id: risks
          title: "Risks & Mitigations"
          required: true
        - id: actions
          title: "Action Items"
          required: false

### Built-in canvas library (`canvases/stdlib/`)

| File | Mode | Description |
|---|---|---|
| `debate.yaml` | auto | Two or more agents argue opposing positions; Moderator synthesises |
| `design-session.yaml` | interactive | Collaborative design with human as decision-maker |
| `code-review-council.yaml` | auto | Multiple reviewers (security, perf, style) assess a diff |
| `architectural-design.yaml` | interactive | ADR-producing session; human holds final call |
| `planning.yaml` | interactive | Sprint/project planning; produces action-item list |

---

## 5. Session Flow

    run(canvas_id, goal)
      |
      CanvasSelector.match(goal) → load canvas
      assemble_participants()
      |
      ┌─────────────────────────────────────────────────────────────┐
      │  Round n  (1 .. max_rounds)                                 │
      │                                                             │
      │  for_each active_persona (sequential on DEBATE channel):   │
      │    build_context()  ← last N msgs from MAIN SESSION        │
      │    + drain mod_whisper queue for this agent                 │
      │    dispatch(TurnRequest)  ← messages.create() Anthropic SDK │
      │      system prompt: assembled at dispatch from:             │
      │        domain context (Fondament primitive | Farga facet)  │
      │        + persona behavioral instructions                    │
      │        + block syntax ([CONSULT],[BTW],[MAIN],[LEAVE])      │
      │        [engine does not inject format rules separately —    │
      │         block syntax lives in the composed definition]      │
      │      message: [{role:user, content: context_string}]        │
      │        context = last N transcript msgs                     │
      │                + pending consultation answers               │
      │                + moderator envelope (if persona=moderator)  │
      │        no prefill — no multi-turn message list              │
      │      model: L1 domain frontmatter                          │
      │             → L2 canvas per-participant override            │
      │             → L3 Moderator [MODEL: x for: y] mid-session    │
      │      response: streamed into block parser                   │                                    │
      │                                                             │
      │    parse response blocks top-to-bottom:                    │
      │                                                             │
      │    [BTW to: X | room]      PUBLIC consultation             │
      │      labeled in transcript → charge main_session           │
      │      X (or room) may respond with their own [BTW]          │
      │                                                             │
      │    [CONSULT to: X]         PRIVATE sidebar                 │
      │      dispatched via CONSULT channel                        │
      │      multiple [CONSULT] blocks → tokio::join_all()         │
      │      result injected into requester context only           │
      │      charge consultations pool                             │
      │                                                             │
      │    [MAIN]                  public turn contribution         │
      │      awaits all [CONSULT] results first                    │
      │      appended to MAIN SESSION transcript                   │
      │      charge main_session pool                              │
      │                                                             │
      │  ── concurrently, while turns execute ──────────────────── │
      │  Moderator reads transcript, sends MOD WHISPER to          │
      │  upcoming agents (non-blocking, queued per agent)          │
      │  ─────────────────────────────────────────────────────────  │
      │                                                             │
      │  if round >= min_rounds:                                   │
      │    moderator turn dispatched like any agent turn           │
      │    receives: transcript, budget state, artifact status     │
      │    parses moderator blocks from response:                  │
      │                                                             │
      │    [INVITE: agent-id]         add to active_personas       │
      │    [RELEASE: agent-id]        remove from rotation         │
      │    [SWITCH_CANVAS: id]        hot-swap rules (v2)          │
      │    [FORK_CONSULTATION: A, B, topic]  private sidebar       │
      │    [ADJUST_BUDGET: pool, Δ]   rebalance pools              │
      │    [REQUEST_APPROVAL: reason] pause → AwaitingApproval     │
      │    [CLOSE]                    trigger synthesis             │
      │                                                             │
      │    if [CLOSE] parsed → break                               │
      │    if [REQUEST_APPROVAL] → pause debate channel            │
      │      human approves  → resume from next turn               │
      │      human rejects   → moderator receives rejection +      │
      │                        reason, emits new moderator turn     │
      │      human modifies  → guidance injected, session resumes  │
      └─────────────────────────────────────────────────────────────┘
      |
      synthesis_phase()
        required output sections → tokio::join_all() each drafted in parallel
        assemble → write_artifacts()
      |
      return SessionOutput

      ── Special states ───────────────────────────────────────────
      AwaitingApproval  (from [REQUEST_APPROVAL: reason])
        debate channel paused, all whispers queued
        human approves  → Resume (next turn)
        human rejects   → moderator gets rejection + reason
                          → new moderator turn emitted
        human modifies  → guidance injected → Resume

      ── Human path (any point) ───────────────────────────────────
      [BTW from: human]   public, authoritative, never overridable
      /call               advisory window (1 turn per agent)
                          human confirms → synthesis_phase() → return

---

## 6. Moderator Role

The Moderator is **a real AI agent dispatched through the same pipeline as every other
participant**. It is not a rule engine. It is not a state machine. Canvas fields are hints
delivered in its context — the Moderator reads them, reasons about the session, and decides
what to do. It can ignore every canvas default. It can reshape the session mid-flight in ways
the canvas never anticipated.

A participant is a **composition address** — `domain × facet × persona` — resolved at
dispatch time. The engine calls Fondament/Farga to assemble the context body, applies the
persona behavioral instructions, and constructs the system prompt dynamically. There is no
fixed agent entity. The same address resolves differently as the underlying context evolves.

Assigning `persona: moderator` grants that participant the moderator block set. The domain
and composition are otherwise unremarkable.

**The engine enforces exactly two things:**
1. `total_tokens` hard limit — tokens cannot be spent that do not exist
2. Human `/call` confirmation — no agent or Moderator can reopen a confirmed close

Everything else is the Moderator’s judgment.

### What the Moderator receives at dispatch

Every Moderator turn is dispatched with a context envelope containing:

    transcript:       last N messages from MAIN SESSION (context_window hint)
    budget_state:
      main_session:   { total, consumed, remaining, pct_remaining }
      consultations:  { total, consumed, remaining, pct_remaining }
      mod_whisper:    { total, consumed, remaining, pct_remaining }
    artifact_status:  { id, title, required, status: missing|draft|complete } per output
    round:            { current, min_hint, max_hint }
    active_personas:  [ { agent_id, role, turns_taken } ]
    canvas_hints:     { convergence_modifier, consultation_max_turns, ... }

Canvas hints are labeled as hints in the envelope. The Moderator decides what to do with them.

### Moderator block set

A Moderator response is parsed for both standard agent blocks and moderator action blocks.
Blocks are executed by the engine; the decision to emit them is entirely the Moderator’s judgment.

| Block | Parameters | Effect |
|---|---|---|
| `[INVITE: agent-id]` | Fondament agent id | Adds agent to active_personas for next round |
| `[RELEASE: agent-id]` | agent id | Removes agent from rotation immediately |
| `[FORK_CONSULTATION: A, B, topic]` | two agent ids + topic | Private sidebar between A and B |
| `[ADJUST_BUDGET: pool, delta]` | pool name + signed token delta | Reallocates between pools |
| `[REQUEST_APPROVAL: reason]` | free text | Pauses debate → AwaitingApproval state |
| `[MODEL: model for: addr]` | model id + participant address | Overrides model for that participant (L3) |
| `[SWITCH_CANVAS: id]` | canvas id | Hot-swaps canvas context (v2 — reserved in v1) |
| `[CLOSE]` | — | Triggers synthesis phase |

A Moderator turn can also include a `[MAIN]` block — the Moderator is a full participant.
Moderator action blocks are parsed after `[CONSULT]` blocks and before `[MAIN]` is committed.

### Calling convention (v1 → v2 path)

v1: block syntax, same parser pipeline as agent blocks. Each block maps to a `ModeratorAction`
enum variant in `moderator.rs`. The parser is the seam — v2 adds MCP tool call dispatch as a
second `ModeratorAction` producer without touching execution logic.

---

## 7. Interaction Primitives

| Primitive | Who | Visibility | Budget pool | Counts as turn? | Overridable? |
|---|---|---|---|---|---|
| `[MAIN]` | any agent | broadcast → transcript | main_session | yes | by Moderator |
| `[BTW to: X\|room]` | any participant | broadcast, labeled | main_session | no | by Moderator |
| `[CONSULT to: X]` | any agent | private 2-turn sidebar | consultations | no | n/a |
| `[LEAVE]` | any agent | broadcast, labeled | — free — | no | Moderator can reject |
| Mod whisper | moderator only | private per-agent queue | mod_whisper | no | n/a |
| Moderator blocks | moderator only | session engine only | — free — | no | n/a |
| Human `[BTW]` | human | broadcast, labeled | main_session | no | **never** |
| Human `/call` | human | broadcast, labeled | — free — | terminal | **never** |

### Human authority rules
1. Human input is always authoritative context.
2. Agents may push back with facts/risks — advisory only.
3. `/call` opens a one-turn advisory window; human then confirms or revises.
4. A confirmed `/call` is a **hard terminal** — no Moderator, convergence signal,
   or budget rule can reopen it.

### [CONSULT] mechanics
- Agent embeds one or more `[CONSULT to: X]` blocks in its response.
- Engine dispatches all concurrently via `tokio::join_all()`.
- Private exchange: max 2 turns, `min_response_tokens` enforced.
- Engine rejects responses below threshold, retries once; on second failure
  closes with `[consultation: refused]` marker.
- Result injected into requester context; transcript only logs header
  `[consultation: A → B]` — Moderator sees header only.
- Pool exhausted → request rejected, agent proceeds without result.

### [LEAVE] mechanics
- Any agent can emit `[LEAVE]` when it judges its contribution to the session complete.
- Engine emits `[participant X has left]` to MAIN SESSION (labeled, visible to room).
- Moderator receives notification and decides: accept (remove from rotation), replace
  (`[INVITE: new-address]`), or reject (whisper to agent requesting it stay).
- If Moderator takes no action within the same round, the leave is accepted by default.

### [BTW] mechanics
- Any participant embeds `[BTW to: X | room]` in their response or sends out-of-turn (human).
- Always labeled in transcript: `[btw from X to Y]` — structurally distinct from [MAIN].
- Recipient responds with their own `[BTW]` block — same label, same pool.
- No token cap on BTW length, but charges main_session pool like any public content.

### Mod whisper mechanics
- Moderator sends whispers asynchronously while the current turn executes.
- Whispers queue in `HashMap<AgentId, VecDeque<WhisperMsg>>`.
- Agent drains its queue into context at the top of `build_context()`, before dispatch.
- Typical triggers: budget threshold (20%, 10%), off-topic drift, verbosity.
- Never visible in transcript.

---

## 8. Transport Trait

    #[async_trait]
    pub trait Transport: Send + Sync {
        async fn broadcast(&self, event: &SessionEvent) -> Result<()>;
        async fn consult(&self, req: &ConsultRequest) -> Result<ConsultResponse>;
        async fn whisper(&self, agent: &AgentId, msg: &WhisperMsg) -> Result<()>;
        async fn recv_human(&self) -> Option<HumanInput>;
        async fn emit_output(&self, output: &SessionOutput) -> Result<()>;
    }

---

## 9. Canvas Auto-Selection

`CanvasSelector` runs before `Initializing`:

1. Reads intake text.
2. Scores against `selector.description`, `selector.tags`, `selector.examples` in each
   stdlib canvas via a lightweight AI call.
3. Returns best match + confidence score.
4. If confidence < 0.7: prompts user to confirm or pick manually.
5. In `auto` mode: low-confidence falls back to `design-session.yaml`.

---

## 10. Project File Structure

    Amassada/
    ├── Cargo.toml
    ├── CLAUDE.md
    ├── crates/
    │   ├── amassada-core/
    │   │   ├── Cargo.toml
    │   │   └── src/
    │   │       ├── lib.rs
    │   │       ├── canvas.rs         YAML parsing + validation
    │   │       ├── selector.rs       intake → canvas matching
    │   │       ├── session.rs        state machine + run() entry point
    │   │       ├── round.rs          round lifecycle
    │   │       ├── turn.rs           turn dispatch + block parser
    │   │       ├── blocks.rs         [MAIN]/[BTW]/[CONSULT] + ModeratorAction parsing
    │   │       ├── budget.rs         pool accounting + threshold triggers
    │   │       ├── context.rs        build_context() sliding window
    │   │       ├── moderator.rs      role detection, ModeratorAction execution
    │   │       ├── synthesis.rs      parallel artifact generation
    │   │       ├── channels/
    │   │       │   ├── mod.rs
    │   │       │   ├── main_session.rs   broadcast transcript
    │   │       │   ├── consult.rs        private sidebar runtime
    │   │       │   └── whisper.rs        per-agent moderator queue
    │   │       └── transport/
    │   │           ├── mod.rs            Transport trait
    │   │           ├── local.rs          LocalTransport
    │   │           └── charradissa.rs    feature = "charradissa"
    │   └── amassada-server/
    │       ├── Cargo.toml
    │       └── src/
    │           ├── main.rs
    │           ├── api.rs            REST endpoints (axum)
    │           └── ws.rs             WebSocket event stream
    └── canvases/
        ├── schema.yaml
        └── stdlib/
            ├── debate.yaml
            ├── design-session.yaml
            ├── code-review-council.yaml
            ├── architectural-design.yaml
            └── planning.yaml

---

## 11. Out of Scope (v1)

- Farga integration (session transcript persistence) — added later as a Transport observer
- Cor plugin canvases — loader accepts an external path; marketplace discovery is future work
- Multi-moderator sessions
- Session resumption / replay
- `[SWITCH_CANVAS]` mid-session — reserved, not executed in v1
- MCP tool call backing for moderator actions — v2; `ModeratorAction` enum is the seam
