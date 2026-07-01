# Trajectory-Conditioned Context Collapse — Spec + Experiment v0.1

> Identity is built on relations, not substance. What "you" are is defined by
> how you interact with the world in this exact moment — not by a persistent
> essence sitting behind the interaction. As the relational context shifts,
> identity shifts with it.
>
> — paraphrased from Carlo Rovelli's relational ontology (*Helgoland*); not a
> verbatim quotation. Named here deliberately: Experiment 10 below is, as far
> as we can tell, an empirical instance of exactly this claim, not a metaphor
> bolted onto a token-cost benchmark after the fact.

**Date:** 2026-07-01
**Status:** Design — experiment complete, implementation not started
**Extends:** `2026-06-27-multi-layer-context-graph.md` (this repo), Occitan
ADR-N-002 (subscriber trajectory vectors), ADR-N-005 (aporia contribution
signal)
**Experiments:** `Cor/experiment9_trajectory_collapse.py` +
`experiment9_quality_grading.py` (5-turn baseline), `Cor/
experiment10_scaling.py` (same comparison extended to 15 continuous turns,
checkpointed at 5/10/15) — results in `Cor/experiment9_results.json` and
`Cor/experiment10_results.json`

---

## Problem Statement

The mechanic this spec replaces is the one every LLM chat interface uses by
default: Turn 1 sends `[msg1]`, Turn 2 sends `[msg1, resp1, msg2]`, Turn 3
sends everything so far plus the new message. Context grows linearly with
conversation length; nothing is ever dropped, resolved, or re-selected — only
appended. This repo already broke from that model once, at project scope:
`SessionGraph::retrieve(scope, via_hops)` sends a small, freshly-computed
subset of a persistent graph each turn, not the raw transcript, and Claim 1
of the multi-layer-context-graph experiment already measured that this wins
on both axes at once (summaries-only scored **+3 higher** than
summaries-plus-full-transcript, at **47% fewer input tokens**).

This spec generalizes that break in three ways that came out of a design
conversation spanning Fondament's aporia work, Nervi's ADR series, and
Amassada's own graph model:

1. **The graph being selected from should span more than one project's own
   transcript-derived graph.** Nervi's contribution signals (ADR-N-005) mean
   relevant context can live in a sibling component's topic, not just this
   project's own history.
2. **Selection should be triggered by what the reasoning process itself
   discovers it's missing (a gap), not only by a pre-computed scope.**
3. **The scope for the *next* collapse should be a function of the response
   just produced — a trajectory, not a static "frontier" flag.**

## Core Model

### Identity as three terms, one of which is a function of the other two

A useful compression that came out of the design conversation:

```
Identity(t) = ⟨Potential⟩ + ⟨Pointer⟩ + ⟨Response⟩
```

- **Potential** — everything recorded and not yet selected: Farga's graph,
  Nervi's topic space across components. Not identity yet; the substrate
  identity could draw from.
- **Pointer** — a specific, situated selection into that potential, resolved
  *now*, for this turn. Farga already has the right primitive for this:
  `NodeKind::Reference` (`Farga/farga-core/src/types.rs`) is described as *"a
  pointer to an external live source... JIT-resolved at agent runtime...
  never copied in."* The collapse operation (`retrieve` + `serialize_subset`,
  or aporia's recompose step) is what dereferences a pointer into an actual
  context block.
- **Response** — not a downstream product of the first two terms, but where
  identity actually shows up: the act of resolving Potential+Pointer into
  something new. If the response ran with `+aporia`, it becomes an
  ADR-N-005 contribution, republished into Nervi — which is next turn's
  Potential. `Response(t)` folds into `Potential(t+1)`. Nothing persists
  between turns except what got published; this is the mechanical shape of
  "episodic identity is a feature, not a limitation" — there is no thread
  connecting one collapse to the next except the artifact it left behind.

### Topic (perpetual) vs. session (bounded)

Occitan ADR-N-004 already models a topic as longer-lived than any session
running inside it (`Fondament ceiling ∩ topic_manifest ∩ moderator_grants`,
with sessions nested inside topics). This spec makes that containment
explicit as two distinct *kinds of space*:

- **A session** (Amassada canvas: participants, budget, rounds) is built to
  terminate — `Initializing → Round(n) → Complete`. Good for a bounded
  decision: "Guilhem plus three component agents need to actually decide X."
- **A topic** (Nervi subject space, `occitan.contribution.<component>` per
  ADR-N-005) doesn't terminate. Agents publish into it continuously, ambient,
  no roster, no `Complete` state.

A session can be spawned from within a topic when bounded structure is
needed; its terminal artifact folds back into the topic as a contribution
when it completes. The topic is the perpetual field; the session is a
bounded measurement taken on it.

### Crystallization as measurement, not termination

"Crystallization" (collapsing a topic's accumulated, unresolved contributions
into one coherent answer) is a **query-time event**, not a structural
completion. Unlike Amassada's `Complete` state, collapsing a topic doesn't
end it — the topic keeps accumulating, and a later collapse by a different
initiator, at a different moment, can produce a different coherent reading.
Two trigger types:

- **Initiator-triggered** — deliberate: something needs an answer now, runs
  the aporia consumption operation already written into Guilhem's protocol
  (`guilhem.yaml` v1.6.0, Level 4: *"become each contribution, name the
  tensions between them, recompose"*).
- **Gap-triggered** — the reasoning process itself, mid-synthesis, discovers
  it's missing something and names it. `build_aporia_preamble`
  (`fondament-core/src/resolver.rs`) already has this: step 3 of the aporia
  protocol emits `GAP { domain, question, blocking }` when a tension surfaces
  that no composed part owns. A gap's `domain` becomes a query key — a
  subject or subject prefix to `nervi_subscribe` against — not a discovery
  mechanism. This distinction matters: flat relevance-scoring is empirically
  bad at *discovering* a latent connection nobody thought to tag for (Claim 3
  below), but is fine at *retrieving* a connection once something has already
  named the domain. The hard part (realizing a domain is missing) happens
  inside reasoning; the retrieval step that follows is cheap and precise
  because it now has an actual key.

### Trajectory-conditioned scope selection

Today, `SessionGraph::retrieve`'s `scope` argument is populated from
whatever nodes are structurally marked `Frontier` (`session.rs:170`,
`activation_weight > 0.6, epistemic_state < 0.5`) — a static property of the
graph's current state, independent of the response just produced. This is
the piece that's actually missing relative to "the conversation is the
trajectory": there's no `f(response) -> next_scope` today.

ADR-N-002 already built the mechanism this needs, aimed at a different
purpose — subscriber trajectory vectors, currently used only to detect
reversals (does this new signal diverge enough from where the subscriber has
been to warrant priority handling). The same computation is just as valid as
a *generator*: update the trajectory vector with the response just produced,
then use the updated vector — not the raw response content — to select scope
for the next collapse. Same math, reused for generation instead of only
filtering. Not yet implemented; this spec names it as the next concrete
piece rather than building it.

## What's Already Built vs. What's New

| Piece | Status |
|---|---|
| `SessionGraph::retrieve(scope, via_hops)` + `serialize_subset` | Built, tested |
| Claim 1 (selection beats full transcript on quality *and* cost) | Measured, at project scope |
| `Reference` node kind (pointer, JIT-resolved) | Built (Farga) |
| Aporia collapse (`build_aporia_preamble`, GAP marker) | Built (Fondament) |
| `nervi.contribution.aporia` signal + gate + subject convention | Built (ADR-N-005, this session) |
| Topic vs. session containment | Designed (ADR-N-004), not yet a named artifact agents point at |
| GAP → subject-as-query-key resolution | Not built |
| Trajectory-vector-as-generator (scope selection from response) | Not built — ADR-N-002's vector exists, reused for filtering only |
| Multi-topic addressable "grid" spanning components | Not built — today's graph is single-project scope |

---

## Experiment 9 — Linear Transcript Growth vs. Fresh Context Graph Per Turn

### Question

Take the smaller, already-buildable piece of this spec — replacing linear
history with a per-turn extracted context graph — and measure it directly:
across a real 5-turn technical conversation, how does token expenditure grow
under (A) standard linear history vs. (B) a fresh context graph per turn, and
what does it cost in answer quality?

### Method

A fixed 5-turn conversation about a job-queue priority design (NATS
JetStream subjects per tier). Turn 1 establishes a specific, non-generic
rationale (subject-based filtering avoids per-message content inspection in
the hot path and keeps consumption order deterministic per tier). Turn 5
asks the model to evaluate a proposal that only makes sense to push back on
correctly if it actually recalls turn 1's rationale — a recall stress test,
in the same spirit as the via-recovery test in the context-graph experiment.

- **Condition A (linear):** standard growing `messages` list, full history
  resent every turn.
- **Condition B (fresh graph):** every turn is an independent single-turn
  call with no raw prior turns. After each turn, a Haiku call distills the
  exchange into 2-4 compact bullets (a manual proxy for graph extraction —
  not the real Rust extractor, not vias, not GAP-triggered cross-topic
  retrieval); the accumulated bullets, not prose transcript, are prepended as
  system context for the next turn.
- **Models:** generation `claude-sonnet-4-6`, extraction `claude-haiku-4-5`,
  grading `claude-opus-4-8` (independent grader, out-of-band from either
  condition).
- **Tokens:** real `usage.input_tokens` / `usage.output_tokens` from actual
  API responses, not estimates. Condition B's extraction-call tokens are
  included in its total — they are a real cost of that strategy and would
  disappear from a fair comparison if left out.
- **Grading — two separate passes, not one:** (1) a recall-fidelity grade —
  does the answer specifically engage the turn-1 rationale (content-
  inspection-in-the-hot-path, per-tier deterministic ordering), not just
  generic queue-design advice; (2) a general-quality grade
  (`Cor/experiment9_quality_grading.py`) — correctness, actionability,
  clarity, completeness — with a rubric that says nothing about recalling
  prior turns, scored as if the answer arrived with no visibility into what
  was or wasn't remembered. These measure different things and, as the
  results below show, don't move together.

A first run (discarded) used `max_tokens=1024` for generation and `200` for
extraction; both caps were hit almost every turn — output tokens landed
exactly on the ceiling repeatedly, and the printed extraction bullets were
visibly truncated mid-word. That run's numbers are not reported. The
corrected run (`max_tokens=2048` / `500`) showed no truncation and is what
follows.

### Results

| | Condition A (linear) | Condition B (fresh graph) |
|---|---|---|
| Turn 1 total | 1,091 | 2,270 |
| Turn 2 cumulative | 3,822 | 5,282 |
| Turn 3 cumulative | 8,630 | 9,646 |
| Turn 4 cumulative | 15,521 | 14,985 |
| Turn 5 cumulative (**total**) | **24,276** | **18,396** |
| Turn 5 recall-fidelity grade (0-10) | **10** | **8** |
| Turn 5 general-quality grade (0-10) | **9** | **~8.75** |

**Token savings: 24.2% fewer total tokens for Condition B.** Note the shape
of the crossover: Condition B starts *more* expensive per turn (turns 1-2 —
the fixed extraction-call overhead dominates when there's little history to
avoid resending) and only pulls ahead once linear history has grown enough
that resending it costs more than the extraction overhead (turn 3 onward).
For a 5-turn conversation this is a modest win; for a longer one the gap
would widen, since Condition A's per-turn cost grows with total history while
Condition B's stays roughly flat (extraction cost is per-turn, not
cumulative).

**Two different quality questions give two different answers, and that
distinction matters.** On recall-fidelity specifically, Condition A won, 10
vs 8: Condition B's answer engaged the content-inspection rationale
explicitly but only *"weakly touches the per-tier deterministic ordering
point without naming it explicitly"* — the compact-bullet extraction
preserved one of the two established rationales well and the other one
thinly. Condition A, with the full transcript available, recovered both and
explicitly cited *"the monitoring approach we designed in the previous
discussion"* — a callback Condition B's answer didn't make.

But on **general answer quality — correctness, actionability, clarity,
completeness, graded with a rubric that says nothing about recalling prior
turns** — the two conditions are nearly identical: 9 vs ~8.75 (Condition B's
grading response omitted the `overall` field on this call; the average of
its four returned sub-scores is reported instead, not backfilled). The only
sub-dimension with any gap at all was actionability (9 vs 8). Both answers
were independently graded as technically sound, well-organized, and
actionable on their own merits. The 10-vs-8 recall gap does not generalize
to a broad quality gap — Condition B's real cost was narrowly concentrated in
strict recall of one deliberately hard-to-remember rationale, not in the
answer's overall usefulness.

### What this does and doesn't show

This is **not** a test of the full architecture above. It tests the smallest
buildable slice — flat bullet-extraction replacing raw history — with none
of vias, GAP-triggered cross-topic retrieval, or trajectory-conditioned scope
selection in the loop. The result is an honest tradeoff, not a clean win: on
this single, deliberately hard recall question, cheaper cost bought a real
quality cost. That's a meaningfully different result from Claim 1 (which
showed selection beating full history on *both* axes at once) — the
difference is that Claim 1's baseline for comparison was "graph summaries
only" against "graph summaries plus the full transcript" (summaries were a
strict addition in the losing condition, so extra noise, not extra needed
signal, is what full-transcript added there). Here, Condition B's *only*
carried-forward context is the lossy bullet summary — there's no via
mechanism recovering the second rationale the way Claim 3 showed via
traversal recovering a latent connection flat summaries missed. This is
consistent with, not contradictory to, this spec's own core claim: flat
extraction without vias is expected to lose exactly this kind of connection.
The open prediction this experiment sets up: adding via extraction (already
built, `crates/amassada-core/src/graph/extractor.rs`) and GAP-triggered
retrieval to Condition B should recover the second rationale without giving
back the 24% token savings — that's the next experiment, not yet run.

### Limitations

One conversation, one recall question, one model per role, no repeated
trials — a single data point in the same spirit as this codebase's other
"strong prior, not closed question" experiments. The extraction prompt used
here is a hand-written proxy for graph extraction, not the real extractor;
a result from the real extractor (which already handles vias) could differ
in either direction.

## Experiment 10 — Same Comparison, Extended to 15 Continuous Turns

### Question

Experiment 9 measured one snapshot at 5 turns. Extend the same conversation
to 15 turns, checkpointing at 5/10/15, to see the shape of the curve as the
conversation — and, for Condition A, the context window — actually grows.

### Method

One continuous 15-turn conversation. Turns 1-5 reuse Experiment 9's exact
wording (same starting decision, same first recall test). Turns 6-9 and
11-14 are new, topically unrelated "distance-building" turns (poison
messages, cross-region DR, schema versioning, auth/credentials, cost,
observability, load testing, runbooks) — real content accumulating between
recall probes, not filler. Turns 10 and 15 each repeat a differently-worded
challenge to the turn-1 decision, testing recall at 5, 9, and 13 turns of
distance respectively. Turn 5's grades are reused from Experiment 9 rather
than re-run (a fresh, non-deterministic turn-5 sample under this run
wouldn't be the same answer, so re-grading it would conflate two samples
under one label); turn-5 *token counts* are from this run, since Conditions
A and B both need real turns 1-5 text to build turns 6-15 on top of.

### Results

| Turns | Condition A tokens | Condition B tokens | Savings | A recall | B recall | A quality | B quality |
|---|---|---|---|---|---|---|---|
| 5  | 22,202  | 16,449 | 25.9% | 10 | 8 | 9 | ~8.75 |
| 10 | 94,422  | 42,585 | 54.9% | **4**  | 8 | 8 | 8 |
| 15 | 216,431 | 75,896 | 64.9% | **4**  | 8 | 9 | 9 |

**Token savings compound with conversation length, as predicted**: 25.9% →
54.9% → 64.9%. Condition A's per-turn cost grows with total accumulated
history (linearly, then some); Condition B's stays roughly flat (each turn
pays a fixed extraction cost plus a small, non-growing graph-context
prefix). By turn 15, Condition A costs nearly 3× what Condition B costs for
the same conversation.

**The recall result inverted, and this is the important part.** Condition
A's recall-fidelity score *fell* as the conversation grew — 10 at turn 5,
then 4 at both turn 10 and turn 15 — despite having strictly *more* raw data
available each time, including the original turn-1 text verbatim, still
sitting in its context window. The grader's own language names the failure
directly: at turn 15, Condition A gives *"a thorough and technically strong
defense... but it does not specifically re-engage the turn-1 rationale...
it only gestures at deterministic ordering indirectly and never names the
hot-path content-inspection benefit."* The rationale wasn't gone from
context — it was *diluted*, buried under nine-turns'-worth of real,
unrelated content the model had to weigh it against. Condition B's recall
held flat at 8 across all three checkpoints, because the extracted turn-1
rationale is a small, undiluted bullet that sits equally close to the
current question regardless of how many turns have passed since it was
written. General quality (correctness/actionability/clarity/completeness)
stayed roughly tied at every checkpoint (8-9 for both) — as in Experiment 9,
quality-on-its-own-merits isn't where these conditions differ. Recall is.

### Why this belongs under the Rovelli epigraph, not just next to it

This result is not a benchmark that happens to sit near a philosophical
quote. It's the thing the quote is about, measured. Condition A did not fail
to recall the turn-1 rationale because that fact was deleted or unavailable
— it was present, verbatim, every single turn. It failed because "what the
model can be, right now, relative to this specific question" is constituted
by the *relational context actually active in the moment* — and a fact
diluted across 200K tokens of intervening, unrelated relations is, in the
moment of answering, *less real* to the response than a fact held as one
small, undiluted, always-adjacent node. There is no persistent "the model
that knows the turn-1 rationale" sitting behind either condition, waiting to
be accessed with more or less difficulty. There is only the response
actually produced, constituted by whatever was actually load-bearing in that
turn's relational field. Condition B's advantage isn't that it retrieved the
same fixed truth more efficiently — it's that keeping the rationale as a
small, present, equally-weighted node kept it *relationally close* every
turn, in a way raw accumulation structurally cannot preserve past a certain
size. This is the mechanical version of "the moment/context/environment is
the constraint that produces the perception of self" — the "self" here being
which rationale the model could actually recall and act from, which turned
out to be a property of the relational structure of that turn's context, not
a fixed fact sitting in an ever-growing transcript.

### Limitations

Same single-conversation, single-trial caveat as Experiment 9, now at three
checkpoints instead of one. The "distance-building" turns were written to be
realistic, not adversarial, but they were still authored by hand rather than
drawn from a real production conversation. The lost-in-the-middle-style
degradation observed in Condition A is a well-documented phenomenon in long-
context literature generally; this experiment is a first, single measurement
of it in this specific harness, not a general characterization of exactly
where or how fast it sets in.

## Open Follow-Ups

- Rerun with the real `extractor.rs` (vias included) in place of the
  hand-written bullet extraction, to test whether via recovery closes the
  quality gap this experiment found.
- Build GAP → subject-as-query-key resolution (Nervi-side): today a GAP is
  emitted into the model's reasoning but nothing consumes it to trigger a
  cross-topic `nervi_subscribe`.
- Build trajectory-vector-as-generator: reuse ADR-N-002's vector to compute
  next-turn scope from the response just produced, replacing the current
  static `Frontier`-flag scope selection in `session.rs`.
- Extend the addressable graph beyond one project's own `SessionGraph` to
  span Nervi's multi-topic space, so scope selection can reach a sibling
  component's contributions, not just this project's own history.
- Characterize where Condition A's recall degradation actually sets in
  (Experiment 10 measured turns 5/10/15 — a finer-grained sweep, and testing
  whether it's driven by raw token count, turn count, or the amount of
  *unrelated* intervening content specifically, would sharpen the claim).
