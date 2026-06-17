# Amassada Mission Engine — Design Spec

**Date:** 2026-06-17  
**Status:** Draft  
**Scope:** `amassada-core` — new `MissionEngine` layer above existing `SessionEngine`

---

## Motivation

The existing `SessionEngine` runs a single structured conversation driven by a canvas YAML. This works well for bounded tasks but cannot handle goals that require a sequence of differently-shaped conversations to converge. A debate needs to feed into a synthesis, which may feed into a decision session — only then is the goal reached.

`MissionEngine` is a new layer above `SessionEngine` that:
- Accepts a goal and a budget ceiling
- Strategizes a pipeline of composable sessions to reach that goal
- Evaluates progress after each session using a lightweight dedicated evaluator
- Adapts the pipeline mid-flight based on what sessions actually produce
- Submits a contribution to Farga on completion as part of the meta-moderator's routine

Canvases become **strategy presets** — named starting points the meta-moderator selects and deviates from — not binding contracts.

---

## New Concepts

### Mission

Top-level unit of work above a Session. `MissionEngine` is the mission — it holds goal, completion condition, sub-objectives, budget, session history, and the runtime fields needed to run it. There is no separate inert `Mission` data struct.

**`goal`** is human-readable intent ("help the team decide on the auth approach").  
**`completion_condition`** is what a lightweight evaluator can judge from artifacts ("a decision doc exists naming one chosen approach with rationale and trade-offs listed").

### SubObjective

A scoped milestone within the mission. Each session targets one or more sub-objectives. The evaluator checks sub-objectives, not the full mission goal, after each session.

```rust
pub struct SubObjective {
    pub id: String,
    pub description: String,
    pub completion_condition: String,       // what evaluator checks against the session artifact
    pub status: SubObjectiveStatus,
    pub output: Option<String>,            // artifact or summary from the session that satisfied it
    pub last_eval_reason: Option<String>,  // evaluator's most recent "why not yet"
}

pub enum SubObjectiveStatus {
    Pending,
    InProgress,
    Complete,
    OutOfScope,   // meta-moderator declared it unreachable within budget
}
```

### MissionBudget

Mission total (hard ceiling) → 20% discretionary (meta-moderator + evaluator) + 80% deployable (sessions).

```rust
pub struct MissionBudget {
    pub total_tokens: u64,
    pub discretionary: u64,                   // 20% of total
    pub discretionary_strategize_spent: u64,  // meta-moderator turns
    pub discretionary_evaluate_spent: u64,    // evaluator turns (Haiku)
    pub deployable: u64,                      // 80% of total
    pub deployable_spent: u64,
}
```

Discretionary spend tracks strategy and evaluation separately because they have different cost profiles — meta-moderator turns are expensive, evaluator turns are cheap (Haiku).

### SessionPlan

What the meta-moderator produces per planned session.

```rust
pub struct SessionPlan {
    pub canvas_id: String,
    pub sub_objective_ids: Vec<String>,
    pub budget_slice: u64,
    pub expected_artifact_description: String,   // meta-moderator's stated expectation
    pub prior_artifact_inject: bool,             // whether to inject the previous session's artifact
}
```

### SessionRecord

The record of an executed session kept by `MissionEngine`.

```rust
pub struct SessionRecord {
    pub session_id: String,
    pub canvas_id: String,
    pub budget_allocated: u64,
    pub budget_spent: u64,
    pub sub_objective_ids: Vec<String>,
    pub artifact: Option<String>,
    pub evaluation: Option<EvaluationResult>,
}

pub struct EvaluationResult {
    pub satisfied: bool,
    pub reason: String,   // always present — "why yes" or "why not yet"
}
```

---

## The Evaluator

The evaluator is a dedicated lightweight component separate from the meta-moderator. It mirrors the `/goal` evaluator in Claude Code: after each session, a small fast model (Haiku) checks whether the completion condition is satisfied by what the session produced.

**The evaluator does not strategize.** It returns a binary judgment and a reason. All strategic decisions (replan, retry, scope change) belong to the meta-moderator, which receives the evaluator's reason as its input.

```rust
pub struct Evaluator {
    model: String,   // defaults to claude-haiku-4-5
    transport: Box<dyn Transport>,
}

impl Evaluator {
    pub async fn check(
        &self,
        condition: &str,
        artifact: &str,
    ) -> Result<EvaluationResult>;
}
```

The evaluator prompt:
```
You are a completion evaluator. Judge whether the following artifact satisfies the condition.

CONDITION: {condition}

ARTIFACT:
{artifact}

Return JSON: { "satisfied": bool, "reason": "one sentence explaining why yes or why not yet" }
```

Evaluator calls draw from `discretionary_evaluate_spent`.

---

## MissionEngine

```rust
pub struct MissionEngine {
    pub mission_id: String,
    pub goal: String,
    pub completion_condition: String,
    pub sub_objectives: Vec<SubObjective>,
    pub budget: MissionBudget,
    pub sessions_run: Vec<SessionRecord>,
    replan_counts: HashMap<String, u32>,   // sub_objective_id → replan attempts
    transport: Box<dyn Transport>,
    evaluator: Evaluator,
}

pub enum MissionState {
    Strategizing,
    Running { session_idx: usize },
    Evaluating { sub_objective_id: String },
    Replanning { reason: String },
    Completing,
    Done,
    Exhausted,
}
```

`SessionEngine` is unchanged. `MissionEngine` instantiates and drives it — `SessionEngine` has no knowledge of the mission above it.

---

## Mission Envelope

Every meta-moderator turn receives the same assembled context injected fresh per call. This is how the stateless Claude API becomes a coherent strategic reasoner across turns.

```
MISSION GOAL
{goal}

COMPLETION CONDITION
{completion_condition}

SUB-OBJECTIVES
  [✓] {id}: {description}
       output: {output_summary}
  [→] {id}: {description}
       last eval: "{last_eval_reason}"
  [ ] {id}: {description}

BUDGET
  deployable: {deployable_spent} / {deployable} tokens used
  discretionary strategize: {discretionary_strategize_spent} / {discretionary} tokens used
  discretionary evaluate: {discretionary_evaluate_spent} / {discretionary} tokens used
  sessions run: {sessions_run.len()}

SESSIONS RUN
  {session_id} ({canvas_id}): [{sub_objective_ids}] — {satisfied? "satisfied" : "not satisfied — " + reason}
  ...
```

---

## Run Loop

```
MissionEngine::run()

1. STRATEGIZING
   Meta-moderator turn (discretionary — strategize).
   Input: mission envelope (empty sessions).
   Output: Vec<SessionPlan>.
   
   The meta-moderator may plan 1–N sessions. It is not required to plan the
   full pipeline upfront — it may return 1–2 sessions with intent to extend
   after seeing results. Plans are pushed to a pending queue.

2. LOOP over pending_plans (front-to-back):

   a. RUNNING
      Guard: budget.deployable_spent + plan.budget_slice > budget.deployable
        → skip to EXHAUSTED.
      
      Optionally inject prior session artifact into goal context
        (if plan.prior_artifact_inject && sessions_run.last().artifact.is_some()).
        Injection format: the prior artifact text is prepended to the session goal as:
          "PRIOR SESSION OUTPUT:\n{artifact}\n\nYOUR GOAL:\n{goal}"
      
      Session goal string: plan.expected_artifact_description (the meta-moderator's stated
        expectation for this session), not the sub-objective description directly. This lets
        the meta-moderator refine the framing per-session when one session targets multiple
        sub-objectives.
      
      SessionEngine::run(canvas, goal=plan.expected_artifact_description, budget=plan.budget_slice)
        → SessionOutput { artifacts, tokens_spent }
      
      Record SessionRecord. Update deployable_spent.

   b. EVALUATING
      For each sub_objective_id in plan.sub_objective_ids:
        Evaluator::check(sub_objective.completion_condition, artifact)
          → EvaluationResult { satisfied, reason }
        
        Update sub_objective.last_eval_reason = reason.
        Update SessionRecord.evaluation.
      
      All satisfied → mark sub-objectives Complete, store artifact, advance.
      Any not satisfied → go to REPLANNING.

   c. REPLANNING
      Guard: replan_count[sub_objective_id] >= 3
        → Meta-moderator receives explicit constraint:
          "Sub-objective {id} has failed evaluation 3 times. Reframe it,
           merge it into another, or mark it OutOfScope."
      
      Meta-moderator turn (discretionary — strategize).
      Input: mission envelope (updated with failure reason).
      Output: Vec<SessionPlan> — replacement or extension for remaining pipeline.
      
      Meta-moderator may:
        - Retry same canvas with a refined goal
        - Swap canvas type (e.g. debate → consultation)
        - Insert a missing intermediate session
        - Mark the sub-objective OutOfScope and continue
      
      Prepend new plans to pending_plans. Increment replan_count. Continue loop.

3. MISSION COMPLETION CHECK
   All sub-objectives Complete or OutOfScope?
     → Evaluator::check(mission.completion_condition, all_artifacts_in_order)
        where all_artifacts_in_order = sessions_run artifacts joined as:
          "SESSION {n} ({canvas_id}):\n{artifact}\n\n---\n\n"
     → satisfied: → COMPLETING
     → not satisfied: meta-moderator extends pipeline (new sub-objectives + plans).
                      Push to pending_plans. Continue loop.

4. COMPLETING
   Meta-moderator final turn (discretionary — strategize).
   Input: full mission envelope + all artifacts.
   Output: FargaVerdict { Submit(FargaContribution) | Skip(reason) }.
   
   Emit MissionEvent::MissionCompleted { verdict, metadata }.
   Transport handles Farga submission (see Farga section).
   State → Done.

5. EXHAUSTED (guard on every state transition)
   deployable budget ceiling hit before mission condition met.
   Meta-moderator synthesizes a partial contribution from completed sub-objectives.
   Emit MissionEvent::MissionExhausted { completed, remaining }.
   Transport handles partial Farga submission.
   State → Done.
```

---

## Farga Completion

On mission completion (full or partial), the meta-moderator synthesizes a Farga contribution and makes a worthiness judgment. Agents already know Farga as their collective memory — submitting to it is the meta-moderator's routine, not a special-cased engine behavior.

### FargaVerdict

```rust
pub enum FargaVerdict {
    Submit { contribution: FargaContribution },
    Skip { reason: String },
}

pub struct FargaContribution {
    pub title: String,
    pub narrative: String,              // synthesized by meta-moderator
    pub artifacts: Vec<SessionArtifact>, // existing type from amassada-core::types
    pub metadata: MissionMetadata,
}

pub struct MissionMetadata {
    pub mission_id: String,
    pub goal: String,
    pub sessions_run: u32,
    pub canvas_types: Vec<String>,
    pub sub_objectives_completed: u32,
    pub total_tokens_spent: u64,
    pub duration_secs: u64,
}
```

### Narrative structure (meta-moderator output)

```
What was sought: {goal}
How it was approached: {sequence of canvas types and why each was chosen}
What was produced: {summary of key artifacts across sessions}
What was decided: {conclusions, outputs, or recommendations}
Why this belongs in collective memory: {meta-moderator's assessment}
```

The meta-moderator returns `Skip` when the output is ephemeral or task-specific (e.g., "draft this email"). Partial missions (Exhausted) always attempt a `Submit` — even incomplete work may be worth preserving.

### Transport integration

`MissionEngine` does not call Farga directly. It emits `MissionEvent::MissionCompleted { verdict, metadata }` via the transport. The transport implementation handles submission:

- **LocalTransport** (CLI): writes artifacts to disk, logs the verdict
- **CharradissaTransport** (Matrix): POSTs to Farga if `Submit`, posts a completion notice to the session room

This keeps `MissionEngine` transport-agnostic, consistent with how `SessionEngine` operates today.

---

## MissionEvents

```rust
pub enum MissionEvent {
    MissionStarted { mission_id: String, goal: String },
    SessionQueued { plan: SessionPlan },
    SessionCompleted { session_id: String, sub_objective_ids: Vec<String> },
    EvaluationFailed { sub_objective_id: String, reason: String },
    Replanning { reason: String },
    MissionCompleted { verdict: FargaVerdict, metadata: MissionMetadata },
    MissionExhausted { completed: Vec<String>, remaining: Vec<String> },  // sub-objective IDs
}
```

---

## What Does Not Change

- `SessionEngine` — unchanged. Receives a canvas, goal string, and budget slice. No mission awareness.
- `Canvas` — unchanged. Becomes a strategy preset the meta-moderator selects via `CanvasLibrary::select()`. The keyword heuristic in `select()` should be upgraded to a lightweight LLM call to match mission context more accurately, but that is a separate change.
- `Transport` trait — extended with `broadcast_mission(MissionEvent)` but otherwise unchanged.
- `ModeratorExecutor` — unchanged. Session-level moderator behavior is unaffected.

---

## Open Questions

1. **Meta-moderator persona** — which Fondament persona does the meta-moderator use? A dedicated `mission-strategist` definition or a promoted `moderator` with an extended system prompt? Decision deferred to implementation.
2. **Parallel sessions** — this spec assumes a linear session pipeline. The meta-moderator could theoretically plan parallel sessions (e.g., two independent research tracks merging into a synthesis). Not in scope for v1.
3. **Human approval gate at mission level** — the existing `AwaitingApproval` mechanism operates at the session level. A mission-level approval gate (e.g., "human must approve the session plan before execution begins") is desirable but deferred to v1.1.
