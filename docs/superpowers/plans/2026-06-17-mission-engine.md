# MissionEngine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `MissionEngine` above `SessionEngine` — a meta-moderator that strategizes a composable session pipeline to reach a goal, evaluates progress after each session via a Haiku evaluator, replans on failure, and submits a Farga contribution on completion.

**Architecture:** `MissionEngine` holds mission state (goal, sub-objectives, budget, session history) and drives `SessionEngine` instances through a pending-plans queue. A `CompletionEvaluator` (Haiku) checks each sub-objective's completion condition after each session; a `MetaModerator` (Opus) strategizes and replans. All three are behind traits for testability without hitting the real API.

**Tech Stack:** Rust, tokio, async-trait, serde_json, reqwest (already in Cargo.toml), uuid, thiserror

---

## File Map

**New files:**
- `crates/amassada-core/src/mission/mod.rs` — re-exports
- `crates/amassada-core/src/mission/types.rs` — all new structs/enums
- `crates/amassada-core/src/mission/envelope.rs` — `build_mission_envelope()`
- `crates/amassada-core/src/mission/evaluator.rs` — `CompletionEvaluator` trait + `ClaudeEvaluator` + `MockEvaluator`
- `crates/amassada-core/src/mission/meta.rs` — `MetaModerator` trait + `ClaudeMetaModerator` + `MockMetaModerator`
- `crates/amassada-core/src/mission/session_runner.rs` — `SessionRunner` trait + `DefaultSessionRunner` + `MockSessionRunner`
- `crates/amassada-core/src/mission/engine.rs` — `MissionEngine` struct + `run()`

**Modified files:**
- `crates/amassada-core/src/error.rs` — add `AmassadaError::Mission(String)`
- `crates/amassada-core/src/lib.rs` — add `pub mod mission`
- `crates/amassada-core/src/session.rs` — change `Box<dyn Transport>` → `Arc<dyn Transport>`

---

## Task 1: Foundation — error variant, lib export, Arc transport

**Files:**
- Modify: `crates/amassada-core/src/error.rs`
- Modify: `crates/amassada-core/src/lib.rs`
- Modify: `crates/amassada-core/src/session.rs`

- [ ] **Step 1: Add `Mission` error variant**

In `error.rs`, add after the `Session` variant:
```rust
    #[error("mission error: {0}")]
    Mission(String),
```

- [ ] **Step 2: Export the mission module**

In `lib.rs`, add after `pub mod session;`:
```rust
pub mod mission;
```

- [ ] **Step 3: Change SessionEngine transport to Arc**

In `session.rs`, change the struct field and constructor:
```rust
use std::sync::Arc;

pub struct SessionEngine {
    pub session_id: String,
    pub canvas: Canvas,
    pub goal: String,
    transport: Arc<dyn Transport>,
}

impl SessionEngine {
    pub fn new(canvas: Canvas, goal: String, transport: Arc<dyn Transport>) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            canvas,
            goal,
            transport,
        }
    }
```

The body of `run()` is unchanged — `self.transport.as_ref()` still works since `Arc<dyn Transport>` derefs.

- [ ] **Step 4: Create mission module stub**

Create `crates/amassada-core/src/mission/mod.rs`:
```rust
pub mod types;
pub mod envelope;
pub mod evaluator;
pub mod meta;
pub mod session_runner;
pub mod engine;
```

- [ ] **Step 5: Verify it compiles**

```bash
cd /Users/bedardpl/project/Amassada
cargo check -p amassada-core
```

Expected: compile errors only for missing modules (types, envelope, etc.) — the imports in mod.rs don't exist yet. That's fine — we'll create them next. If you get errors _other_ than "file not found for module", fix them now.

Actually: temporarily comment out all lines in `mod.rs` except `pub mod types;` and create a stub `types.rs` with just `// stub` to verify the chain. Then uncomment as you add each file.

- [ ] **Step 6: Commit**

```bash
git add crates/amassada-core/src/error.rs \
        crates/amassada-core/src/lib.rs \
        crates/amassada-core/src/session.rs \
        crates/amassada-core/src/mission/mod.rs
git commit -m "feat: mission module scaffold — Arc transport, Mission error, module tree"
```

---

## Task 2: Mission types

**Files:**
- Create: `crates/amassada-core/src/mission/types.rs`

- [ ] **Step 1: Write types.rs**

```rust
use serde::{Deserialize, Serialize};
use crate::types::OutputArtifact;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubObjectiveStatus {
    Pending,
    InProgress,
    Complete,
    OutOfScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubObjective {
    pub id: String,
    pub description: String,
    pub completion_condition: String,
    pub status: SubObjectiveStatus,
    pub output: Option<String>,
    pub last_eval_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionBudget {
    pub total_tokens: u64,
    pub discretionary: u64,
    pub discretionary_strategize_spent: u64,
    pub discretionary_evaluate_spent: u64,
    pub deployable: u64,
    pub deployable_spent: u64,
}

impl MissionBudget {
    pub fn new(total_tokens: u64) -> Self {
        let discretionary = total_tokens / 5;
        let deployable = total_tokens - discretionary;
        Self {
            total_tokens,
            discretionary,
            discretionary_strategize_spent: 0,
            discretionary_evaluate_spent: 0,
            deployable,
            deployable_spent: 0,
        }
    }

    pub fn discretionary_remaining(&self) -> u64 {
        let spent = self.discretionary_strategize_spent + self.discretionary_evaluate_spent;
        self.discretionary.saturating_sub(spent)
    }

    pub fn deployable_remaining(&self) -> u64 {
        self.deployable.saturating_sub(self.deployable_spent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPlan {
    pub canvas_id: String,
    pub sub_objective_ids: Vec<String>,
    pub budget_slice: u64,
    pub expected_artifact_description: String,
    pub prior_artifact_inject: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub satisfied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub canvas_id: String,
    pub budget_allocated: u64,
    pub budget_spent: u64,
    pub sub_objective_ids: Vec<String>,
    pub artifact: Option<String>,
    pub evaluation: Option<EvaluationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionMetadata {
    pub mission_id: String,
    pub goal: String,
    pub sessions_run: u32,
    pub canvas_types: Vec<String>,
    pub sub_objectives_completed: u32,
    pub total_tokens_spent: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FargaContribution {
    pub title: String,
    pub narrative: String,
    pub artifacts: Vec<OutputArtifact>,
    pub metadata: MissionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FargaVerdict {
    Submit { contribution: FargaContribution },
    Skip { reason: String },
}

pub struct MissionOutcome {
    pub mission_id: String,
    pub exhausted: bool,
    pub completed_sub_objective_ids: Vec<String>,
    pub verdict: FargaVerdict,
    pub metadata: MissionMetadata,
}
```

- [ ] **Step 2: Write unit tests (inline)**

At the bottom of `types.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_splits_20_80() {
        let b = MissionBudget::new(100_000);
        assert_eq!(b.discretionary, 20_000);
        assert_eq!(b.deployable, 80_000);
        assert_eq!(b.total_tokens, 100_000);
    }

    #[test]
    fn budget_remaining_tracks_spend() {
        let mut b = MissionBudget::new(100_000);
        b.discretionary_strategize_spent = 5_000;
        b.discretionary_evaluate_spent = 2_000;
        assert_eq!(b.discretionary_remaining(), 13_000);
        b.deployable_spent = 30_000;
        assert_eq!(b.deployable_remaining(), 50_000);
    }

    #[test]
    fn budget_remaining_saturates_at_zero() {
        let mut b = MissionBudget::new(10_000);
        b.deployable_spent = 99_000;
        assert_eq!(b.deployable_remaining(), 0);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p amassada-core mission::types
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/amassada-core/src/mission/types.rs
git commit -m "feat: mission types — SubObjective, MissionBudget, SessionPlan, FargaVerdict"
```

---

## Task 3: Envelope builder

**Files:**
- Create: `crates/amassada-core/src/mission/envelope.rs`

- [ ] **Step 1: Write the failing test**

At the bottom of `envelope.rs` (write the test first, `build_mission_envelope` stub comes next):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::*;

    fn make_engine_state() -> EnvelopeState {
        EnvelopeState {
            goal: "decide on auth approach".into(),
            completion_condition: "a decision doc exists naming one chosen approach".into(),
            sub_objectives: vec![
                SubObjective {
                    id: "obj-1".into(),
                    description: "surface trade-offs".into(),
                    completion_condition: "pros/cons listed for at least two options".into(),
                    status: SubObjectiveStatus::Complete,
                    output: Some("JWT vs sessions: JWT stateless but harder to revoke...".into()),
                    last_eval_reason: None,
                },
                SubObjective {
                    id: "obj-2".into(),
                    description: "pick one approach".into(),
                    completion_condition: "one option chosen with rationale".into(),
                    status: SubObjectiveStatus::Pending,
                    output: None,
                    last_eval_reason: Some("no decision made yet".into()),
                },
            ],
            budget: {
                let mut b = MissionBudget::new(100_000);
                b.deployable_spent = 10_000;
                b.discretionary_strategize_spent = 2_000;
                b
            },
            sessions_run: vec![
                SessionRecord {
                    session_id: "s1".into(),
                    canvas_id: "debate".into(),
                    budget_allocated: 10_000,
                    budget_spent: 9_800,
                    sub_objective_ids: vec!["obj-1".into()],
                    artifact: Some("JWT vs sessions comparison...".into()),
                    evaluation: Some(EvaluationResult { satisfied: true, reason: "pros/cons listed".into() }),
                },
            ],
        }
    }

    #[test]
    fn envelope_contains_all_sections() {
        let state = make_engine_state();
        let env = build_mission_envelope(&state);
        assert!(env.contains("MISSION GOAL"));
        assert!(env.contains("decide on auth approach"));
        assert!(env.contains("COMPLETION CONDITION"));
        assert!(env.contains("SUB-OBJECTIVES"));
        assert!(env.contains("[✓] obj-1"));
        assert!(env.contains("[ ] obj-2"));
        assert!(env.contains("last eval: \"no decision made yet\""));
        assert!(env.contains("BUDGET"));
        assert!(env.contains("10000 / 80000"));
        assert!(env.contains("SESSIONS RUN"));
        assert!(env.contains("s1 (debate)"));
        assert!(env.contains("satisfied"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::envelope
```

Expected: compile error — `build_mission_envelope` and `EnvelopeState` don't exist.

- [ ] **Step 3: Implement envelope.rs**

```rust
use crate::mission::types::{MissionBudget, SessionRecord, SubObjective, SubObjectiveStatus};

/// Snapshot of MissionEngine state passed to build_mission_envelope.
/// Using a struct (not a reference to MissionEngine) avoids a circular dep
/// between envelope.rs and engine.rs.
pub struct EnvelopeState<'a> {
    pub goal: &'a str,
    pub completion_condition: &'a str,
    pub sub_objectives: &'a [SubObjective],
    pub budget: &'a MissionBudget,
    pub sessions_run: &'a [SessionRecord],
}

pub fn build_mission_envelope(state: &EnvelopeState<'_>) -> String {
    let mut buf = String::new();

    buf.push_str(&format!("MISSION GOAL\n{}\n\n", state.goal));
    buf.push_str(&format!("COMPLETION CONDITION\n{}\n\n", state.completion_condition));

    buf.push_str("SUB-OBJECTIVES\n");
    for obj in state.sub_objectives {
        let marker = match obj.status {
            SubObjectiveStatus::Complete  => "✓",
            SubObjectiveStatus::InProgress => "→",
            SubObjectiveStatus::OutOfScope => "✗",
            SubObjectiveStatus::Pending   => " ",
        };
        buf.push_str(&format!("  [{}] {}: {}\n", marker, obj.id, obj.description));
        if let Some(out) = &obj.output {
            buf.push_str(&format!("       output: {}\n", truncate(out, 200)));
        }
        if let Some(reason) = &obj.last_eval_reason {
            buf.push_str(&format!("       last eval: \"{}\"\n", reason));
        }
    }

    buf.push('\n');
    buf.push_str("BUDGET\n");
    buf.push_str(&format!(
        "  deployable: {} / {} tokens used\n",
        state.budget.deployable_spent, state.budget.deployable
    ));
    buf.push_str(&format!(
        "  discretionary strategize: {} / {} tokens used\n",
        state.budget.discretionary_strategize_spent, state.budget.discretionary
    ));
    buf.push_str(&format!(
        "  discretionary evaluate: {} / {} tokens used\n",
        state.budget.discretionary_evaluate_spent, state.budget.discretionary
    ));
    buf.push_str(&format!("  sessions run: {}\n", state.sessions_run.len()));

    if !state.sessions_run.is_empty() {
        buf.push_str("\nSESSIONS RUN\n");
        for record in state.sessions_run {
            let status = match &record.evaluation {
                Some(e) if e.satisfied => "satisfied".to_string(),
                Some(e) => format!("not satisfied — {}", e.reason),
                None => "pending evaluation".to_string(),
            };
            buf.push_str(&format!(
                "  {} ({}): [{}] — {}\n",
                record.session_id,
                record.canvas_id,
                record.sub_objective_ids.join(", "),
                status,
            ));
        }
    }

    buf
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

// test module written above (write tests first per TDD)
#[cfg(test)]
mod tests {
    // ... (paste the test block from Step 1 here)
}
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::envelope
```

Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/envelope.rs
git commit -m "feat: mission envelope builder"
```

---

## Task 4: CompletionEvaluator trait + MockEvaluator

**Files:**
- Create: `crates/amassada-core/src/mission/evaluator.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_evaluator_returns_queued_results() {
        let mock = MockEvaluator::new(vec![
            EvaluationResult { satisfied: false, reason: "not done yet".into() },
            EvaluationResult { satisfied: true,  reason: "condition met".into() },
        ]);
        let r1 = mock.check("condition", "artifact").await.unwrap();
        assert!(!r1.satisfied);
        assert_eq!(r1.reason, "not done yet");
        let r2 = mock.check("condition", "artifact").await.unwrap();
        assert!(r2.satisfied);
    }

    #[tokio::test]
    async fn mock_evaluator_defaults_to_pass_when_empty() {
        let mock = MockEvaluator::new(vec![]);
        let r = mock.check("condition", "artifact").await.unwrap();
        assert!(r.satisfied);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::evaluator
```

Expected: compile error — types don't exist yet.

- [ ] **Step 3: Implement evaluator.rs (trait + mock only)**

```rust
use async_trait::async_trait;
use std::sync::Mutex;
use crate::error::Result;
use crate::mission::types::EvaluationResult;

#[async_trait]
pub trait CompletionEvaluator: Send + Sync {
    async fn check(&self, condition: &str, artifact: &str) -> Result<EvaluationResult>;
}

pub struct MockEvaluator {
    responses: Mutex<Vec<EvaluationResult>>,
}

impl MockEvaluator {
    pub fn new(responses: Vec<EvaluationResult>) -> Self {
        Self { responses: Mutex::new(responses) }
    }
}

#[async_trait]
impl CompletionEvaluator for MockEvaluator {
    async fn check(&self, _condition: &str, _artifact: &str) -> Result<EvaluationResult> {
        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            Ok(EvaluationResult { satisfied: true, reason: "mock: default pass".into() })
        } else {
            Ok(queue.remove(0))
        }
    }
}

#[cfg(test)]
mod tests { /* paste Step 1 test block here */ }
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::evaluator
```

Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/evaluator.rs
git commit -m "feat: CompletionEvaluator trait + MockEvaluator"
```

---

## Task 5: ClaudeEvaluator

**Files:**
- Modify: `crates/amassada-core/src/mission/evaluator.rs`

- [ ] **Step 1: Add ClaudeEvaluator to evaluator.rs**

Add after `MockEvaluator`'s impl block:
```rust
use crate::dispatch::{dispatch, TurnRequest};
use crate::error::AmassadaError;

pub struct ClaudeEvaluator {
    pub model: String,
}

impl Default for ClaudeEvaluator {
    fn default() -> Self {
        Self { model: "claude-haiku-4-5-20251001".into() }
    }
}

#[async_trait]
impl CompletionEvaluator for ClaudeEvaluator {
    async fn check(&self, condition: &str, artifact: &str) -> Result<EvaluationResult> {
        let system = "You are a completion evaluator. \
            Judge whether the artifact satisfies the condition. \
            Respond ONLY with valid JSON, no markdown: \
            {\"satisfied\": bool, \"reason\": \"one sentence explaining why yes or why not yet\"}";

        let context = format!("CONDITION:\n{condition}\n\nARTIFACT:\n{artifact}");

        let resp = dispatch(TurnRequest {
            system_prompt: system.into(),
            context,
            model: self.model.clone(),
            max_tokens: 256,
        }).await?;

        let val: serde_json::Value = serde_json::from_str(resp.text.trim())
            .map_err(|e| AmassadaError::Mission(format!("evaluator JSON parse: {e}")))?;

        Ok(EvaluationResult {
            satisfied: val["satisfied"].as_bool().unwrap_or(false),
            reason: val["reason"].as_str().unwrap_or("(no reason)").to_string(),
        })
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p amassada-core
```

Expected: no errors.

- [ ] **Step 3: Manual smoke test (requires ANTHROPIC_API_KEY)**

In an integration test or scratch binary, call:
```rust
let eval = ClaudeEvaluator::default();
let result = eval.check(
    "the text mentions a chosen option by name",
    "After deliberation, we chose JWT tokens for their statelessness.",
).await.unwrap();
assert!(result.satisfied, "reason: {}", result.reason);
```

Run: `ANTHROPIC_API_KEY=<key> cargo test -p amassada-core -- --ignored claude_evaluator_smoke`

Expected: satisfied=true, reason mentions the chosen option.

- [ ] **Step 4: Commit**

```bash
git add crates/amassada-core/src/mission/evaluator.rs
git commit -m "feat: ClaudeEvaluator — Haiku completion check"
```

---

## Task 6: MetaModerator trait + MockMetaModerator

**Files:**
- Create: `crates/amassada-core/src/mission/meta.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::{FargaContribution, FargaVerdict, MissionMetadata, SessionPlan};

    fn make_plan(canvas_id: &str) -> SessionPlan {
        SessionPlan {
            canvas_id: canvas_id.into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "trade-off analysis".into(),
            prior_artifact_inject: false,
        }
    }

    fn make_metadata() -> MissionMetadata {
        MissionMetadata {
            mission_id: "m1".into(),
            goal: "decide".into(),
            sessions_run: 1,
            canvas_types: vec!["debate".into()],
            sub_objectives_completed: 1,
            total_tokens_spent: 10_000,
            duration_secs: 30,
        }
    }

    #[tokio::test]
    async fn mock_meta_returns_queued_plans() {
        let mock = MockMetaModerator::new(
            vec![vec![make_plan("debate")], vec![make_plan("design-session")]],
            FargaVerdict::Skip { reason: "ephemeral".into() },
        );
        let (plans, tokens) = mock.strategize("envelope").await.unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].canvas_id, "debate");
        assert_eq!(tokens, 0);
        let (plans2, _) = mock.replan("envelope").await.unwrap();
        assert_eq!(plans2[0].canvas_id, "design-session");
    }

    #[tokio::test]
    async fn mock_meta_returns_configured_verdict() {
        let mock = MockMetaModerator::new(
            vec![],
            FargaVerdict::Skip { reason: "ephemeral".into() },
        );
        let (verdict, _) = mock.complete("envelope", "artifacts").await.unwrap();
        assert!(matches!(verdict, FargaVerdict::Skip { .. }));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::meta
```

Expected: compile error.

- [ ] **Step 3: Implement meta.rs (trait + mock only)**

```rust
use async_trait::async_trait;
use std::sync::Mutex;
use crate::error::Result;
use crate::mission::types::{FargaVerdict, SessionPlan};

/// Returns (output, tokens_used) so MissionEngine can track discretionary spend.
#[async_trait]
pub trait MetaModerator: Send + Sync {
    async fn strategize(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn replan(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn complete(&self, envelope: &str, artifacts_summary: &str) -> Result<(FargaVerdict, u64)>;
}

pub struct MockMetaModerator {
    plan_queue: Mutex<Vec<Vec<SessionPlan>>>,
    verdict: FargaVerdict,
}

impl MockMetaModerator {
    pub fn new(plan_batches: Vec<Vec<SessionPlan>>, verdict: FargaVerdict) -> Self {
        Self {
            plan_queue: Mutex::new(plan_batches),
            verdict,
        }
    }
}

#[async_trait]
impl MetaModerator for MockMetaModerator {
    async fn strategize(&self, _envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let mut q = self.plan_queue.lock().unwrap();
        Ok((if q.is_empty() { vec![] } else { q.remove(0) }, 0))
    }

    async fn replan(&self, _envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let mut q = self.plan_queue.lock().unwrap();
        Ok((if q.is_empty() { vec![] } else { q.remove(0) }, 0))
    }

    async fn complete(&self, _envelope: &str, _artifacts: &str) -> Result<(FargaVerdict, u64)> {
        Ok((self.verdict.clone(), 0))
    }
}

#[cfg(test)]
mod tests { /* paste Step 1 test block here */ }
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::meta
```

Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/meta.rs
git commit -m "feat: MetaModerator trait + MockMetaModerator"
```

---

## Task 7: ClaudeMetaModerator

**Files:**
- Modify: `crates/amassada-core/src/mission/meta.rs`

- [ ] **Step 1: Add constants and parse functions**

Add before `MockMetaModerator`:
```rust
use crate::dispatch::{dispatch, TurnRequest};
use crate::error::AmassadaError;
use crate::mission::types::{FargaContribution, MissionMetadata, OutputArtifact};

const META_SYSTEM_STRATEGIZE: &str = "\
You are a mission strategist for a multi-agent session engine. \
You receive a mission envelope describing a goal, completion condition, sub-objectives, and budget.

Your job: produce a JSON array of session plans that will satisfy pending sub-objectives.

Each plan is a JSON object:
{
  \"canvas_id\": \"<one of: debate, design-session, code-review-council, architectural-design, planning>\",
  \"sub_objective_ids\": [\"<id>\", ...],
  \"budget_slice\": <tokens to allocate — must not exceed deployable remaining>,
  \"expected_artifact_description\": \"<what this session should produce>\",
  \"prior_artifact_inject\": <true|false — true injects the previous session artifact into this session goal>
}

Rules:
- You may plan 1 to N sessions. You do not need to plan the full pipeline now.
- Sum of budget_slice values must not exceed the deployable budget remaining shown in the envelope.
- Respond ONLY with a JSON array. No prose, no markdown code fences.";

const META_SYSTEM_REPLAN: &str = "\
You are a mission strategist. A session failed its evaluation — the failure reason is in the envelope.

Produce replacement session plans as a JSON array (same schema as before).
Options: retry same canvas with refined goal, swap canvas type, insert missing intermediate session, or return an empty array to mark the sub-objective as out-of-scope.

Respond ONLY with a JSON array. No prose, no markdown code fences.";

const META_SYSTEM_COMPLETE: &str = "\
You are a mission strategist completing a mission. Review the envelope and all produced artifacts.

Decide if this output is worth adding to collective memory (Farga). Skip if ephemeral or task-specific.

Respond ONLY with valid JSON (no markdown):
{
  \"verdict\": \"submit\" | \"skip\",
  \"title\": \"<short title, if submit>\",
  \"narrative\": \"<What was sought. How approached. What was produced. What was decided. Why it belongs in collective memory. — if submit>\",
  \"skip_reason\": \"<reason — if skip>\"
}";

fn parse_session_plans(text: &str) -> Result<Vec<SessionPlan>> {
    serde_json::from_str(text.trim())
        .map_err(|e| AmassadaError::Mission(format!("meta strategize parse: {e} | raw: {text}")))
}

fn parse_farga_verdict(text: &str, dummy_artifacts: Vec<OutputArtifact>) -> Result<FargaVerdict> {
    let val: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| AmassadaError::Mission(format!("meta complete parse: {e} | raw: {text}")))?;

    match val["verdict"].as_str() {
        Some("submit") => Ok(FargaVerdict::Submit {
            contribution: FargaContribution {
                title: val["title"].as_str().unwrap_or("Mission Complete").to_string(),
                narrative: val["narrative"].as_str().unwrap_or("").to_string(),
                artifacts: dummy_artifacts,
                metadata: MissionMetadata {
                    mission_id: String::new(),
                    goal: String::new(),
                    sessions_run: 0,
                    canvas_types: vec![],
                    sub_objectives_completed: 0,
                    total_tokens_spent: 0,
                    duration_secs: 0,
                },
            },
        }),
        _ => Ok(FargaVerdict::Skip {
            reason: val["skip_reason"].as_str().unwrap_or("meta chose to skip").to_string(),
        }),
    }
}
```

Note: `parse_farga_verdict` takes `dummy_artifacts` because the real artifacts and metadata are set by `MissionEngine::run()` after calling `meta.complete()` — the meta-moderator only supplies the narrative content. `MissionEngine` fills in the rest.

- [ ] **Step 2: Add ClaudeMetaModerator struct**

```rust
pub struct ClaudeMetaModerator {
    pub model: String,
}

impl Default for ClaudeMetaModerator {
    fn default() -> Self {
        Self { model: "claude-opus-4-8".into() }
    }
}

#[async_trait]
impl MetaModerator for ClaudeMetaModerator {
    async fn strategize(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_STRATEGIZE.into(),
            context: envelope.into(),
            model: self.model.clone(),
            max_tokens: 2048,
        }).await?;
        let plans = parse_session_plans(&resp.text)?;
        Ok((plans, (resp.input_tokens + resp.output_tokens) as u64))
    }

    async fn replan(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_REPLAN.into(),
            context: envelope.into(),
            model: self.model.clone(),
            max_tokens: 2048,
        }).await?;
        let plans = parse_session_plans(&resp.text)?;
        Ok((plans, (resp.input_tokens + resp.output_tokens) as u64))
    }

    async fn complete(&self, envelope: &str, artifacts_summary: &str) -> Result<(FargaVerdict, u64)> {
        let context = format!("{envelope}\n\nALL ARTIFACTS:\n{artifacts_summary}");
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_COMPLETE.into(),
            context,
            model: self.model.clone(),
            max_tokens: 4096,
        }).await?;
        let verdict = parse_farga_verdict(&resp.text, vec![])?;
        Ok((verdict, (resp.input_tokens + resp.output_tokens) as u64))
    }
}
```

- [ ] **Step 3: Verify compiles**

```bash
cargo check -p amassada-core
```

Expected: clean.

- [ ] **Step 4: Manual smoke test (requires ANTHROPIC_API_KEY)**

```rust
#[tokio::test]
#[ignore]
async fn claude_meta_strategize_smoke() {
    let meta = ClaudeMetaModerator::default();
    let envelope = "MISSION GOAL\nDecide on auth approach\n\nCOMPLETION CONDITION\nOne approach chosen with rationale\n\nSUB-OBJECTIVES\n  [ ] obj-1: Surface trade-offs\n       completion_condition: pros/cons listed for two options\n\nBUDGET\n  deployable: 0 / 80000 tokens used\n  sessions run: 0\n";
    let (plans, tokens) = meta.strategize(envelope).await.unwrap();
    assert!(!plans.is_empty(), "expected at least one plan");
    assert!(tokens > 0);
    println!("Plans: {:?}", plans);
}
```

Run: `ANTHROPIC_API_KEY=<key> cargo test -p amassada-core -- --ignored claude_meta_strategize_smoke`

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/meta.rs
git commit -m "feat: ClaudeMetaModerator — Opus strategize/replan/complete"
```

---

## Task 8: SessionRunner trait + DefaultSessionRunner + MockSessionRunner

**Files:**
- Create: `crates/amassada-core/src/mission/session_runner.rs`

`SessionRunner` exists to make `MissionEngine` testable without real Claude API calls. `DefaultSessionRunner` wraps `SessionEngine`. `MockSessionRunner` returns pre-baked `SessionOutput` values.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OutputArtifact, SessionOutput};

    fn make_output(artifact_content: &str) -> SessionOutput {
        SessionOutput {
            session_id: "s1".into(),
            canvas_id: "debate".into(),
            goal: "test goal".into(),
            artifacts: vec![OutputArtifact {
                id: "a1".into(),
                title: "Result".into(),
                content: artifact_content.into(),
                required: true,
            }],
            total_tokens: 5_000,
        }
    }

    #[tokio::test]
    async fn mock_runner_returns_queued_outputs() {
        let runner = MockSessionRunner::new(vec![
            make_output("first artifact"),
            make_output("second artifact"),
        ]);
        let canvas = crate::canvas::Canvas::from_yaml(
            "id: debate\nversion: \"1\"\nmode: auto\nselector:\n  description: d\n  tags: []\n  examples: []\ninitial_participants: []\nbudget:\n  total_tokens: 10000\n  pools:\n    main_session: 8000\n    consultations: 1500\n    mod_whisper: 500\nconsultation:\n  max_turns: 3\n  min_response_tokens: 50\nrounds:\n  min: 1\n  max: 5\n  convergence_modifier: 0.8\n  context_window: 8192\nhuman:\n  slot: false\noutput:\n  format: markdown\n  sections: []"
        ).unwrap();
        let out1 = runner.run(canvas.clone(), "goal 1".into()).await.unwrap();
        assert_eq!(out1.artifacts[0].content, "first artifact");
        let out2 = runner.run(canvas, "goal 2".into()).await.unwrap();
        assert_eq!(out2.artifacts[0].content, "second artifact");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::session_runner
```

Expected: compile error.

- [ ] **Step 3: Implement session_runner.rs**

```rust
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use crate::canvas::Canvas;
use crate::error::Result;
use crate::session::SessionEngine;
use crate::transport::Transport;
use crate::types::SessionOutput;

#[async_trait]
pub trait SessionRunner: Send + Sync {
    async fn run(&self, canvas: Canvas, goal: String) -> Result<SessionOutput>;
}

/// Scales canvas budget pools proportionally to a new total, then runs a real SessionEngine.
pub struct DefaultSessionRunner {
    transport: Arc<dyn Transport>,
}

impl DefaultSessionRunner {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl SessionRunner for DefaultSessionRunner {
    async fn run(&self, mut canvas: Canvas, goal: String) -> Result<SessionOutput> {
        SessionEngine::new(canvas, goal, Arc::clone(&self.transport))
            .run()
            .await
    }
}

/// Scale canvas budget to a given token ceiling (used by MissionEngine before calling runner).
pub fn scale_canvas_budget(mut canvas: Canvas, budget_tokens: u64) -> Canvas {
    let orig = canvas.budget.total_tokens as u64;
    if orig == 0 || budget_tokens == orig {
        return canvas;
    }
    let scale = budget_tokens as f64 / orig as f64;
    canvas.budget.total_tokens = budget_tokens.min(u32::MAX as u64) as u32;
    canvas.budget.pools.main_session =
        ((canvas.budget.pools.main_session as f64 * scale) as u32).max(1);
    canvas.budget.pools.consultations =
        ((canvas.budget.pools.consultations as f64 * scale) as u32).max(1);
    canvas.budget.pools.mod_whisper =
        ((canvas.budget.pools.mod_whisper as f64 * scale) as u32).max(1);
    canvas
}

pub struct MockSessionRunner {
    outputs: Mutex<Vec<SessionOutput>>,
}

impl MockSessionRunner {
    pub fn new(outputs: Vec<SessionOutput>) -> Self {
        Self { outputs: Mutex::new(outputs) }
    }
}

#[async_trait]
impl SessionRunner for MockSessionRunner {
    async fn run(&self, _canvas: Canvas, _goal: String) -> Result<SessionOutput> {
        let mut q = self.outputs.lock().unwrap();
        Ok(if q.is_empty() {
            SessionOutput {
                session_id: uuid::Uuid::new_v4().to_string(),
                canvas_id: "mock".into(),
                goal: _goal,
                artifacts: vec![],
                total_tokens: 0,
            }
        } else {
            q.remove(0)
        })
    }
}

#[cfg(test)]
mod tests { /* paste Step 1 test block here */ }
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::session_runner
```

Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/session_runner.rs
git commit -m "feat: SessionRunner trait + Default/Mock impls + scale_canvas_budget"
```

---

## Task 9: MissionEngine struct + new()

**Files:**
- Create: `crates/amassada-core/src/mission/engine.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::{FargaVerdict, SubObjective, SubObjectiveStatus};
    use crate::mission::evaluator::MockEvaluator;
    use crate::mission::meta::MockMetaModerator;
    use crate::mission::session_runner::MockSessionRunner;

    fn make_sub_obj(id: &str) -> SubObjective {
        SubObjective {
            id: id.into(),
            description: format!("{id} description"),
            completion_condition: format!("{id} is done"),
            status: SubObjectiveStatus::Pending,
            output: None,
            last_eval_reason: None,
        }
    }

    fn make_engine() -> MissionEngine {
        MissionEngine::new(
            "decide auth approach".into(),
            "one approach chosen with rationale".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(MockSessionRunner::new(vec![])),
            Box::new(MockEvaluator::new(vec![])),
            Box::new(MockMetaModerator::new(vec![], FargaVerdict::Skip { reason: "test".into() })),
        )
    }

    #[test]
    fn new_initializes_budget_correctly() {
        let engine = make_engine();
        assert_eq!(engine.budget.total_tokens, 100_000);
        assert_eq!(engine.budget.discretionary, 20_000);
        assert_eq!(engine.budget.deployable, 80_000);
        assert_eq!(engine.sessions_run.len(), 0);
        assert_eq!(engine.sub_objectives.len(), 1);
        assert!(engine.replan_counts.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::engine
```

Expected: compile error — `MissionEngine` doesn't exist.

- [ ] **Step 3: Implement engine.rs struct + new()**

```rust
use std::collections::HashMap;
use uuid::Uuid;
use crate::error::Result;
use crate::mission::types::*;
use crate::mission::evaluator::CompletionEvaluator;
use crate::mission::meta::MetaModerator;
use crate::mission::session_runner::SessionRunner;

pub struct MissionEngine {
    pub mission_id: String,
    pub goal: String,
    pub completion_condition: String,
    pub sub_objectives: Vec<SubObjective>,
    pub budget: MissionBudget,
    pub sessions_run: Vec<SessionRecord>,
    pub(crate) replan_counts: HashMap<String, u32>,
    runner: Box<dyn SessionRunner>,
    evaluator: Box<dyn CompletionEvaluator>,
    meta: Box<dyn MetaModerator>,
}

impl MissionEngine {
    pub fn new(
        goal: String,
        completion_condition: String,
        sub_objectives: Vec<SubObjective>,
        total_budget_tokens: u64,
        runner: Box<dyn SessionRunner>,
        evaluator: Box<dyn CompletionEvaluator>,
        meta: Box<dyn MetaModerator>,
    ) -> Self {
        Self {
            mission_id: Uuid::new_v4().to_string(),
            goal,
            completion_condition,
            sub_objectives,
            budget: MissionBudget::new(total_budget_tokens),
            sessions_run: Vec::new(),
            replan_counts: HashMap::new(),
            runner,
            evaluator,
            meta,
        }
    }
}

#[cfg(test)]
mod tests { /* paste Step 1 test block here */ }
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::engine
```

Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/engine.rs
git commit -m "feat: MissionEngine struct + new()"
```

---

## Task 10: run() — happy path (strategize → session → evaluate → complete)

**Files:**
- Modify: `crates/amassada-core/src/mission/engine.rs`

Happy path: meta returns 1 plan, session runs, evaluator says satisfied, all sub-objectives complete, mission condition satisfied, meta returns Submit verdict.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `engine.rs`:
```rust
    use crate::canvas::Canvas;
    use crate::mission::session_runner::MockSessionRunner;
    use crate::mission::types::{EvaluationResult, FargaVerdict, SessionPlan};
    use crate::types::{OutputArtifact, SessionOutput};

    fn stub_canvas_yaml() -> &'static str {
        "id: debate\nversion: \"1\"\nmode: auto\nselector:\n  description: d\n  tags: []\n  examples: []\ninitial_participants: []\nbudget:\n  total_tokens: 10000\n  pools:\n    main_session: 8000\n    consultations: 1500\n    mod_whisper: 500\nconsultation:\n  max_turns: 3\n  min_response_tokens: 50\nrounds:\n  min: 1\n  max: 5\n  convergence_modifier: 0.8\n  context_window: 8192\nhuman:\n  slot: false\noutput:\n  format: markdown\n  sections: []"
    }

    fn stub_session_output(content: &str) -> SessionOutput {
        SessionOutput {
            session_id: "s1".into(),
            canvas_id: "debate".into(),
            goal: "test".into(),
            artifacts: vec![OutputArtifact {
                id: "a1".into(), title: "Result".into(),
                content: content.into(), required: true,
            }],
            total_tokens: 5_000,
        }
    }

    #[tokio::test]
    async fn happy_path_single_session_completes_mission() {
        let plan = SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "pros/cons analysis".into(),
            prior_artifact_inject: false,
        };

        let canvas = Canvas::from_yaml(stub_canvas_yaml()).unwrap();
        let runner = MockSessionRunner::new(vec![stub_session_output("JWT wins: stateless, scalable.")]);
        let evaluator = MockEvaluator::new(vec![
            EvaluationResult { satisfied: true, reason: "analysis present".into() }, // sub-obj check
            EvaluationResult { satisfied: true, reason: "mission condition met".into() }, // mission check
        ]);
        let meta = MockMetaModerator::new(
            vec![vec![plan]],
            FargaVerdict::Submit {
                contribution: crate::mission::types::FargaContribution {
                    title: "Auth decision".into(),
                    narrative: "We chose JWT.".into(),
                    artifacts: vec![],
                    metadata: MissionMetadata {
                        mission_id: String::new(), goal: String::new(),
                        sessions_run: 1, canvas_types: vec!["debate".into()],
                        sub_objectives_completed: 1, total_tokens_spent: 10_000, duration_secs: 0,
                    },
                }
            },
        );

        let mut engine = MissionEngine::new(
            "decide auth".into(),
            "one approach chosen".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(runner),
            Box::new(evaluator),
            Box::new(meta),
        );

        // Need CanvasLibrary — inject via engine or pass canvas directly.
        // For test we use engine.run_with_canvas_lookup_fn or we pre-populate library.
        // See implementation note below.
        let outcome = engine.run(|id| {
            if id == "debate" { Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap()) }
            else { None }
        }).await.unwrap();

        assert!(!outcome.exhausted);
        assert_eq!(outcome.completed_sub_objective_ids, vec!["obj-1"]);
        assert!(matches!(outcome.verdict, FargaVerdict::Submit { .. }));
        assert_eq!(engine.sessions_run.len(), 1);
        assert_eq!(engine.budget.deployable_spent, 5_000);
    }
```

**Implementation note on canvas lookup:** rather than embedding `CanvasLibrary` in `MissionEngine` (which would require a real filesystem), `run()` takes a `canvas_lookup: impl Fn(&str) -> Option<Canvas>`. This keeps `MissionEngine` testable without loading YAML files.

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::engine::tests::happy_path
```

Expected: compile error — `run()` doesn't exist.

- [ ] **Step 3: Implement run() up through COMPLETING**

Add to `engine.rs` (inside `impl MissionEngine`):
```rust
    pub async fn run(
        &mut self,
        canvas_lookup: impl Fn(&str) -> Option<Canvas>,
    ) -> Result<MissionOutcome> {
        use crate::mission::envelope::{build_mission_envelope, EnvelopeState};
        use crate::mission::session_runner::scale_canvas_budget;
        use std::time::Instant;

        let started_at = Instant::now();

        // STRATEGIZING
        let envelope = build_mission_envelope(&EnvelopeState {
            goal: &self.goal,
            completion_condition: &self.completion_condition,
            sub_objectives: &self.sub_objectives,
            budget: &self.budget,
            sessions_run: &self.sessions_run,
        });
        let (mut pending_plans, tokens) = self.meta.strategize(&envelope).await?;
        self.budget.discretionary_strategize_spent += tokens;

        // LOOP
        while !pending_plans.is_empty() {
            let plan = pending_plans.remove(0);

            // Budget guard
            if plan.budget_slice > self.budget.deployable_remaining() {
                return self.build_outcome(FargaVerdict::Skip {
                    reason: "budget exhausted before plan could run".into()
                }, true, started_at.elapsed().as_secs());
            }

            // Resolve canvas
            let canvas = canvas_lookup(&plan.canvas_id)
                .ok_or_else(|| crate::error::AmassadaError::CanvasNotFound(plan.canvas_id.clone()))?;
            let canvas = scale_canvas_budget(canvas, plan.budget_slice);

            // Build session goal — with optional prior artifact injection
            let session_goal = if plan.prior_artifact_inject {
                match self.sessions_run.last().and_then(|r| r.artifact.as_ref()) {
                    Some(prior) => format!(
                        "PRIOR SESSION OUTPUT:\n{prior}\n\nYOUR GOAL:\n{}",
                        plan.expected_artifact_description
                    ),
                    None => plan.expected_artifact_description.clone(),
                }
            } else {
                plan.expected_artifact_description.clone()
            };

            // RUNNING
            let output = self.runner.run(canvas.clone(), session_goal).await?;
            let artifact_text = output.artifacts.iter()
                .map(|a| format!("# {}\n{}", a.title, a.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            let tokens_spent = output.total_tokens as u64;
            self.budget.deployable_spent += tokens_spent;

            // EVALUATING — check each targeted sub-objective
            let mut all_satisfied = true;
            let mut first_fail_reason = String::new();

            for obj_id in &plan.sub_objective_ids {
                if let Some(obj) = self.sub_objectives.iter_mut().find(|o| &o.id == obj_id) {
                    if obj.status == SubObjectiveStatus::Complete {
                        continue;
                    }
                    obj.status = SubObjectiveStatus::InProgress;
                    let eval = self.evaluator.check(&obj.completion_condition, &artifact_text).await?;
                    self.budget.discretionary_evaluate_spent += 50; // Haiku eval est.
                    obj.last_eval_reason = Some(eval.reason.clone());

                    if eval.satisfied {
                        obj.status = SubObjectiveStatus::Complete;
                        obj.output = Some(artifact_text.clone());
                    } else {
                        all_satisfied = false;
                        if first_fail_reason.is_empty() {
                            first_fail_reason = format!("{}: {}", obj_id, eval.reason);
                        }
                    }
                }
            }

            // Record session
            let eval_result = if all_satisfied {
                Some(EvaluationResult { satisfied: true, reason: "all sub-objectives met".into() })
            } else {
                Some(EvaluationResult { satisfied: false, reason: first_fail_reason.clone() })
            };
            self.sessions_run.push(SessionRecord {
                session_id: output.session_id.clone(),
                canvas_id: canvas.id.clone(),
                budget_allocated: plan.budget_slice,
                budget_spent: tokens_spent,
                sub_objective_ids: plan.sub_objective_ids.clone(),
                artifact: Some(artifact_text.clone()),
                evaluation: eval_result,
            });

            if !all_satisfied {
                // REPLANNING — handled in Task 11
                let replan_result = self.do_replan(&first_fail_reason, &plan.sub_objective_ids).await?;
                pending_plans.splice(0..0, replan_result);
                continue;
            }
        }

        // MISSION COMPLETION CHECK
        let all_artifacts = self.sessions_run.iter()
            .enumerate()
            .filter_map(|(i, r)| r.artifact.as_ref().map(|a| format!("SESSION {} ({}):\n{}", i + 1, r.canvas_id, a)))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let mission_eval = self.evaluator.check(&self.completion_condition, &all_artifacts).await?;
        self.budget.discretionary_evaluate_spent += 50;

        if !mission_eval.satisfied {
            // Out of plans and mission not met — return exhausted
            return self.build_outcome(
                FargaVerdict::Skip { reason: format!("mission condition not met: {}", mission_eval.reason) },
                true,
                started_at.elapsed().as_secs(),
            );
        }

        // COMPLETING
        let envelope = build_mission_envelope(&EnvelopeState {
            goal: &self.goal,
            completion_condition: &self.completion_condition,
            sub_objectives: &self.sub_objectives,
            budget: &self.budget,
            sessions_run: &self.sessions_run,
        });
        let (mut verdict, tokens) = self.meta.complete(&envelope, &all_artifacts).await?;
        self.budget.discretionary_strategize_spent += tokens;

        // Patch metadata into Submit verdict
        if let FargaVerdict::Submit { ref mut contribution } = verdict {
            let all_oa: Vec<_> = self.sessions_run.iter()
                .flat_map(|r| r.artifact.iter().map(|a| crate::types::OutputArtifact {
                    id: r.session_id.clone(),
                    title: r.canvas_id.clone(),
                    content: a.clone(),
                    required: false,
                }))
                .collect();
            contribution.artifacts = all_oa;
            contribution.metadata = self.build_metadata(started_at.elapsed().as_secs());
        }

        self.build_outcome(verdict, false, started_at.elapsed().as_secs())
    }

    fn build_metadata(&self, duration_secs: u64) -> MissionMetadata {
        MissionMetadata {
            mission_id: self.mission_id.clone(),
            goal: self.goal.clone(),
            sessions_run: self.sessions_run.len() as u32,
            canvas_types: self.sessions_run.iter().map(|r| r.canvas_id.clone()).collect(),
            sub_objectives_completed: self.sub_objectives.iter()
                .filter(|o| o.status == SubObjectiveStatus::Complete)
                .count() as u32,
            total_tokens_spent: self.budget.deployable_spent
                + self.budget.discretionary_strategize_spent
                + self.budget.discretionary_evaluate_spent,
            duration_secs,
        }
    }

    fn build_outcome(
        &self,
        verdict: FargaVerdict,
        exhausted: bool,
        duration_secs: u64,
    ) -> Result<MissionOutcome> {
        Ok(MissionOutcome {
            mission_id: self.mission_id.clone(),
            exhausted,
            completed_sub_objective_ids: self.sub_objectives.iter()
                .filter(|o| o.status == SubObjectiveStatus::Complete)
                .map(|o| o.id.clone())
                .collect(),
            verdict,
            metadata: self.build_metadata(duration_secs),
        })
    }

    async fn do_replan(
        &mut self,
        reason: &str,
        _failed_obj_ids: &[String],
    ) -> Result<Vec<SessionPlan>> {
        // Implemented in Task 11 — stub here
        let _ = reason;
        Ok(vec![])
    }
```

Also add the required `use` imports at the top of `engine.rs`:
```rust
use crate::canvas::Canvas;
use crate::mission::types::{EvaluationResult, MissionOutcome, SubObjectiveStatus};
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::engine::tests::happy_path
```

Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/engine.rs
git commit -m "feat: MissionEngine run() — happy path strategize→session→evaluate→complete"
```

---

## Task 11: run() — replan path

**Files:**
- Modify: `crates/amassada-core/src/mission/engine.rs`

- [ ] **Step 1: Write the failing test**

Add to `tests` module in `engine.rs`:
```rust
    #[tokio::test]
    async fn replan_fires_on_failed_evaluation() {
        let plan_1 = SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "first attempt".into(),
            prior_artifact_inject: false,
        };
        let plan_2 = SessionPlan {
            canvas_id: "design-session".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "second attempt with clearer framing".into(),
            prior_artifact_inject: true,
        };

        let runner = MockSessionRunner::new(vec![
            stub_session_output("vague output"),
            stub_session_output("JWT chosen for statelessness."),
        ]);
        let evaluator = MockEvaluator::new(vec![
            EvaluationResult { satisfied: false, reason: "no decision made".into() }, // obj-1 first pass
            EvaluationResult { satisfied: true,  reason: "decision present".into() }, // obj-1 second pass
            EvaluationResult { satisfied: true,  reason: "mission met".into() },      // mission check
        ]);
        let meta = MockMetaModerator::new(
            vec![vec![plan_1], vec![plan_2]],  // strategize returns plan_1, replan returns plan_2
            FargaVerdict::Skip { reason: "test".into() },
        );

        let mut engine = MissionEngine::new(
            "decide auth".into(),
            "one approach chosen".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(runner),
            Box::new(evaluator),
            Box::new(meta),
        );

        let outcome = engine.run(|id| {
            Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap())
        }).await.unwrap();

        assert!(!outcome.exhausted);
        assert_eq!(engine.sessions_run.len(), 2);
        assert_eq!(*engine.replan_counts.get("obj-1").unwrap(), 1);
        assert_eq!(outcome.completed_sub_objective_ids, vec!["obj-1"]);
    }

    #[tokio::test]
    async fn replan_limit_marks_out_of_scope() {
        // obj-1 fails 3 times → should be marked OutOfScope after REPLAN_LIMIT
        let make_plan = |desc: &str| SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 5_000,
            expected_artifact_description: desc.into(),
            prior_artifact_inject: false,
        };

        let runner = MockSessionRunner::new(vec![
            stub_session_output("bad 1"),
            stub_session_output("bad 2"),
            stub_session_output("bad 3"),
        ]);
        // 3 sub-obj fails, then nothing left → mission not met → exhausted verdict
        let evaluator = MockEvaluator::new(vec![
            EvaluationResult { satisfied: false, reason: "nope".into() },
            EvaluationResult { satisfied: false, reason: "nope".into() },
            EvaluationResult { satisfied: false, reason: "nope".into() },
            EvaluationResult { satisfied: false, reason: "mission not met".into() },
        ]);
        // Meta returns one plan per replan (3 replans before hitting limit)
        let meta = MockMetaModerator::new(
            vec![
                vec![make_plan("attempt 1")],
                vec![make_plan("attempt 2")],
                vec![make_plan("attempt 3")],
            ],
            FargaVerdict::Skip { reason: "exhausted".into() },
        );

        let mut engine = MissionEngine::new(
            "decide".into(),
            "one approach chosen".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(runner),
            Box::new(evaluator),
            Box::new(meta),
        );

        let outcome = engine.run(|_| Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap())).await.unwrap();

        assert!(outcome.exhausted || engine.sub_objectives[0].status == SubObjectiveStatus::OutOfScope);
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::engine::tests::replan
```

Expected: `replan_fires_on_failed_evaluation` fails because `do_replan` is a stub returning `vec![]`.

- [ ] **Step 3: Implement do_replan()**

Replace the stub `do_replan` with:
```rust
    const REPLAN_LIMIT: u32 = 3;

    async fn do_replan(
        &mut self,
        reason: &str,
        failed_obj_ids: &[String],
    ) -> Result<Vec<SessionPlan>> {
        use crate::mission::envelope::{build_mission_envelope, EnvelopeState};

        // Increment replan counts for all failed objectives
        for obj_id in failed_obj_ids {
            let count = self.replan_counts.entry(obj_id.clone()).or_insert(0);
            *count += 1;

            if *count >= Self::REPLAN_LIMIT {
                // Mark as out of scope
                if let Some(obj) = self.sub_objectives.iter_mut().find(|o| &o.id == obj_id) {
                    obj.status = SubObjectiveStatus::OutOfScope;
                }
            }
        }

        // If all failing objectives are now out of scope, no replan needed
        let still_pending = failed_obj_ids.iter().any(|id| {
            self.sub_objectives.iter().any(|o| &o.id == id && o.status != SubObjectiveStatus::OutOfScope)
        });
        if !still_pending {
            return Ok(vec![]);
        }

        let envelope = build_mission_envelope(&EnvelopeState {
            goal: &self.goal,
            completion_condition: &self.completion_condition,
            sub_objectives: &self.sub_objectives,
            budget: &self.budget,
            sessions_run: &self.sessions_run,
        });
        let (new_plans, tokens) = self.meta.replan(&envelope).await?;
        self.budget.discretionary_strategize_spent += tokens;
        Ok(new_plans)
    }
```

- [ ] **Step 4: Run tests to verify pass**

```bash
cargo test -p amassada-core mission::engine::tests
```

Expected: all tests pass including the two new replan tests.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/engine.rs
git commit -m "feat: MissionEngine replan path + REPLAN_LIMIT → OutOfScope"
```

---

## Task 12: run() — budget exhaustion guard

**Files:**
- Modify: `crates/amassada-core/src/mission/engine.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn budget_exhaustion_before_second_session() {
        let plan_1 = SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 75_000,  // leaves 5k of 80k deployable
            expected_artifact_description: "analysis".into(),
            prior_artifact_inject: false,
        };
        let plan_2 = SessionPlan {
            canvas_id: "design-session".into(),
            sub_objective_ids: vec!["obj-2".into()],
            budget_slice: 10_000,  // exceeds remaining 5k → exhausted
            expected_artifact_description: "decision".into(),
            prior_artifact_inject: false,
        };

        let runner = MockSessionRunner::new(vec![
            stub_session_output("partial work done"),
        ]);
        let evaluator = MockEvaluator::new(vec![
            EvaluationResult { satisfied: true, reason: "obj-1 done".into() },
        ]);
        let meta = MockMetaModerator::new(
            vec![vec![plan_1, plan_2]],
            FargaVerdict::Skip { reason: "exhausted".into() },
        );

        let mut engine = MissionEngine::new(
            "decide".into(), "all done".into(),
            vec![make_sub_obj("obj-1"), make_sub_obj("obj-2")],
            100_000,
            Box::new(runner), Box::new(evaluator), Box::new(meta),
        );

        let outcome = engine.run(|_| Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap())).await.unwrap();

        assert!(outcome.exhausted);
        assert_eq!(engine.sessions_run.len(), 1);  // only 1 session ran
        assert_eq!(outcome.completed_sub_objective_ids, vec!["obj-1"]);
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p amassada-core mission::engine::tests::budget_exhaustion
```

Expected: test fails — the guard logic in `run()` returns early on budget exhaustion, but the existing implementation might not behave correctly for the case where plan 2 is already in the initial plan list.

The existing `run()` already has a budget guard at the top of the loop:
```rust
if plan.budget_slice > self.budget.deployable_remaining() {
    return self.build_outcome(FargaVerdict::Skip { ... }, true, ...);
}
```

This should work. If the test fails, check whether `deployable_remaining()` correctly reflects `budget_slice=75_000` spent after session 1 completes (output.total_tokens).

MockSessionRunner returns `total_tokens: 5_000` in `stub_session_output`. So after session 1: `deployable_spent = 5_000`, remaining = 75_000. Plan 2 wants 10_000 → 10_000 ≤ 75_000 → no exhaustion!

Fix: change `stub_session_output` in the exhaustion test to return `total_tokens: 75_000`:
```rust
        let runner = MockSessionRunner::new(vec![
            SessionOutput {
                session_id: "s1".into(), canvas_id: "debate".into(), goal: "test".into(),
                artifacts: vec![OutputArtifact { id: "a1".into(), title: "R".into(), content: "partial work done".into(), required: true }],
                total_tokens: 75_000,
            }
        ]);
```

- [ ] **Step 3: Fix the test (no code change needed, just the test data)**

Update the test to use `total_tokens: 75_000` as shown in Step 2.

- [ ] **Step 4: Run test to verify pass**

```bash
cargo test -p amassada-core mission::engine::tests
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amassada-core/src/mission/engine.rs
git commit -m "test: budget exhaustion guard for MissionEngine run loop"
```

---

## Task 13: Integration test + cargo check clean

**Files:**
- Modify: `crates/amassada-core/src/mission/engine.rs` (test only)
- Verify: `crates/amassada-core/src/` (all files compile cleanly)

- [ ] **Step 1: Run full test suite**

```bash
cargo test -p amassada-core
```

Expected: all tests pass, no warnings about unused imports.

- [ ] **Step 2: Verify cargo check clean**

```bash
cargo check -p amassada-core 2>&1 | grep -E "^error"
```

Expected: no output (zero errors).

- [ ] **Step 3: Write end-to-end integration test**

Add a new test at the bottom of `engine.rs` tests module:
```rust
    #[tokio::test]
    async fn e2e_two_session_mission_with_replan() {
        // obj-1: first session fails eval, replan swaps canvas, second session satisfies
        // obj-2: satisfied by second session artifact
        // mission condition: satisfied by combined artifacts
        let plan_initial = SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 15_000,
            expected_artifact_description: "surface trade-offs".into(),
            prior_artifact_inject: false,
        };
        let plan_replan = SessionPlan {
            canvas_id: "design-session".into(),
            sub_objective_ids: vec!["obj-1".into(), "obj-2".into()],
            budget_slice: 20_000,
            expected_artifact_description: "structured decision with rationale".into(),
            prior_artifact_inject: true,
        };

        let runner = MockSessionRunner::new(vec![
            stub_session_output("JWT vs sessions: JWT stateless but hard to revoke."),
            stub_session_output("Decision: JWT. Rationale: stateless for our scale."),
        ]);
        let evaluator = MockEvaluator::new(vec![
            // Session 1 → obj-1 check: fail
            EvaluationResult { satisfied: false, reason: "no decision made, only comparison".into() },
            // Session 2 → obj-1 check: pass
            EvaluationResult { satisfied: true, reason: "decision with rationale present".into() },
            // Session 2 → obj-2 check: pass
            EvaluationResult { satisfied: true, reason: "rationale documented".into() },
            // Mission condition check: pass
            EvaluationResult { satisfied: true, reason: "both objectives met".into() },
        ]);
        let meta = MockMetaModerator::new(
            vec![vec![plan_initial], vec![plan_replan]],
            FargaVerdict::Submit {
                contribution: crate::mission::types::FargaContribution {
                    title: "Auth decision: JWT".into(),
                    narrative: "Team decided on JWT after a comparison and structured decision session.".into(),
                    artifacts: vec![],
                    metadata: MissionMetadata {
                        mission_id: String::new(), goal: String::new(),
                        sessions_run: 2, canvas_types: vec!["debate".into(), "design-session".into()],
                        sub_objectives_completed: 2, total_tokens_spent: 0, duration_secs: 0,
                    },
                }
            },
        );

        let mut engine = MissionEngine::new(
            "decide on auth approach".into(),
            "one approach chosen with rationale".into(),
            vec![make_sub_obj("obj-1"), make_sub_obj("obj-2")],
            100_000,
            Box::new(runner),
            Box::new(evaluator),
            Box::new(meta),
        );

        let outcome = engine.run(|_| Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap())).await.unwrap();

        assert!(!outcome.exhausted, "should not be exhausted");
        assert_eq!(engine.sessions_run.len(), 2);
        assert_eq!(*engine.replan_counts.get("obj-1").unwrap(), 1);
        assert_eq!(outcome.completed_sub_objective_ids.len(), 2);
        assert!(matches!(outcome.verdict, FargaVerdict::Submit { .. }));

        // Metadata is patched from engine state
        assert_eq!(outcome.metadata.sessions_run, 2);
        assert_eq!(outcome.metadata.sub_objectives_completed, 2);
    }
```

- [ ] **Step 4: Run integration test**

```bash
cargo test -p amassada-core mission::engine::tests::e2e
```

Expected: passes.

- [ ] **Step 5: Final commit**

```bash
git add crates/amassada-core/src/mission/
git commit -m "feat: MissionEngine complete — mission module with evaluator, meta-moderator, run loop, tests"
```

---

## Self-Review

**Spec coverage:**
- [x] MissionEngine struct with goal, completion_condition, sub_objectives, budget, sessions_run, replan_counts — Task 9
- [x] MissionBudget (20%/80% split, separate strategize/evaluate tracking) — Task 2
- [x] SubObjective with completion_condition and last_eval_reason — Task 2
- [x] SessionPlan with prior_artifact_inject — Task 2
- [x] EvaluationResult — Task 2
- [x] CompletionEvaluator trait (Haiku) — Tasks 4-5
- [x] MetaModerator trait (Opus) — Tasks 6-7
- [x] Mission envelope with all sections — Task 3
- [x] Run loop: strategize → session → evaluate → replan or advance — Tasks 10-11
- [x] Replan guard at REPLAN_LIMIT (3) → OutOfScope — Task 11
- [x] Budget exhaustion guard → MissionOutcome.exhausted — Task 12
- [x] Mission completion check (separate evaluator call on all artifacts) — Task 10
- [x] COMPLETING: meta.complete() → FargaVerdict, metadata patched from engine state — Task 10
- [x] FargaVerdict.Submit vs Skip — Task 2
- [x] SessionRunner trait for testability — Task 8
- [x] scale_canvas_budget for budget overrides — Task 8
- [x] Artifact injection format (PRIOR SESSION OUTPUT: / YOUR GOAL:) — Task 10
- [x] All artifacts joined as "SESSION N (canvas_id):\n..." for mission eval — Task 10
- [x] OutOfScope status — Task 11
- [x] SessionEngine unchanged except Arc transport — Task 1

**Open questions deferred from spec (not implemented here):**
- Meta-moderator persona from Fondament (ClaudeMetaModerator uses generic system prompt — production upgrade)
- Parallel sessions (linear pipeline only in v1)
- Human approval gate at mission level
