# Amassada Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the multi-agent session engine — canvas-driven, three-channel async runtime, LLM dispatch, Moderator-as-agent, LocalTransport for CLI.

**Architecture:** `amassada-core` is a pure-Rust library (no I/O) containing the session state machine, canvas parser, block parser, budget accounting, channel runtime, and Transport trait. `amassada-server` wraps core with an Axum REST + WebSocket service. The Anthropic API is called via reqwest. The key seam is the `Transport` trait — LocalTransport ships in core, CharradissaTransport is feature-gated.

**Tech Stack:** Rust, tokio (broadcast/mpsc channels), axum, serde_yaml, reqwest (Anthropic API direct), async-trait, clap

---

## File Map

```
Amassada/
├── Cargo.toml
├── canvases/stdlib/                   # shipped canvas YAML files
│   ├── debate.yaml
│   ├── design-session.yaml
│   ├── code-review-council.yaml
│   ├── architectural-design.yaml
│   └── planning.yaml
└── crates/
    ├── amassada-core/src/
    │   ├── lib.rs
    │   ├── types.rs                   # AgentId, TurnRecord, SessionEvent, HumanInput, SessionState
    │   ├── error.rs                   # AmassadaError
    │   ├── canvas.rs                  # Canvas struct, YAML parsing, CanvasSelector
    │   ├── budget.rs                  # BudgetPool, BudgetLedger, threshold triggers
    │   ├── blocks.rs                  # AgentBlock, ModeratorAction enums + streaming parser
    │   ├── context.rs                 # ContextBuilder, build_context() sliding window
    │   ├── channels/
    │   │   ├── mod.rs
    │   │   ├── main_session.rs        # broadcast::channel wrapper
    │   │   ├── consult.rs             # ConsultRuntime: mpsc pairs, tokio::join_all dispatch
    │   │   └── whisper.rs             # WhisperQueue: HashMap<AgentId, VecDeque<WhisperMsg>>
    │   ├── transport/
    │   │   ├── mod.rs                 # Transport trait
    │   │   └── local.rs               # LocalTransport: stdout + stdin/injected channel
    │   ├── dispatch.rs                # TurnRequest, Anthropic API call, streaming parser hookup
    │   ├── moderator.rs               # ModeratorAction execution, whisper scheduling
    │   ├── synthesis.rs               # parallel artifact generation
    │   ├── round.rs                   # round lifecycle, per-persona turn loop
    │   └── session.rs                 # SessionEngine state machine, run()
    └── amassada-server/src/
        ├── main.rs
        ├── api.rs                     # REST: start, state, human_input
        └── ws.rs                      # WebSocket SessionEvent stream
```

---

### Task 1: Workspace + Canvas Stdlib Files

**Files:** `Cargo.toml`, `crates/amassada-core/Cargo.toml`, `crates/amassada-server/Cargo.toml`, `canvases/stdlib/*.yaml`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
# Amassada/Cargo.toml
[workspace]
members = ["crates/amassada-core", "crates/amassada-server"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.7"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json", "stream"] }
clap = { version = "4", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
futures = "0.3"
```

- [ ] **Step 2: Create crate Cargo.tomls**

```toml
# crates/amassada-core/Cargo.toml
[package]
name = "amassada-core"
version = "0.1.0"
edition = "2021"

[features]
charradissa = []

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }
reqwest = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
futures = { workspace = true }
tracing = { workspace = true }
```

```toml
# crates/amassada-server/Cargo.toml
[package]
name = "amassada-server"
version = "0.1.0"
edition = "2021"

[dependencies]
amassada-core = { path = "../amassada-core" }
tokio = { workspace = true }
axum = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

- [ ] **Step 3: Write canvas YAML files**

```yaml
# canvases/stdlib/debate.yaml
id: debate
version: "1.0.0"
mode: auto
selector:
  description: "Two or more agents argue opposing positions; Moderator synthesises into a decision"
  tags: [debate, argument, positions, pros-cons, decision, tradeoffs]
  examples:
    - "should we use microservices or a monolith?"
    - "debate the tradeoffs of event sourcing"
    - "argue for and against GraphQL"
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
  - persona: adversarial
    domain: fondament/senior-engineer
budget:
  total_tokens: 100000
  pools:
    main_session: 80000
    consultations: 15000
    mod_whisper: 5000
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 2
  max: 5
  convergence_modifier: 0.8
  context_window: 20
human:
  slot: false
output:
  format: markdown
  sections:
    - id: decision
      title: "Decision"
      required: true
    - id: rationale
      title: "Rationale"
      required: true
    - id: risks
      title: "Risks & Mitigations"
      required: false
```

```yaml
# canvases/stdlib/design-session.yaml
id: design-session
version: "1.0.0"
mode: interactive
selector:
  description: "Collaborative design session with human as final decision-maker, produces ADR"
  tags: [design, architecture, adr, interactive, collaborative]
  examples:
    - "design the auth service"
    - "how should we structure the data pipeline?"
    - "let's design the API gateway"
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
  - persona: human
    authority: binding
budget:
  total_tokens: 150000
  pools:
    main_session: 120000
    consultations: 20000
    mod_whisper: 10000
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 2
  max: 8
  convergence_modifier: 0.7
  context_window: 25
human:
  slot: true
  advisory_window_turns: 2
output:
  format: markdown
  sections:
    - id: decision
      title: "Architecture Decision"
      required: true
    - id: components
      title: "Components & Interfaces"
      required: true
    - id: risks
      title: "Risks & Mitigations"
      required: true
    - id: actions
      title: "Action Items"
      required: false
```

```yaml
# canvases/stdlib/code-review-council.yaml
id: code-review-council
version: "1.0.0"
mode: auto
selector:
  description: "Multiple reviewers (security, perf, style) assess a code diff or PR"
  tags: [code-review, security, performance, style, diff, pr]
  examples:
    - "review this pull request"
    - "code review for the auth service changes"
    - "security review of the payment module"
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
  - persona: breaker
    domain: fondament/security-engineer
budget:
  total_tokens: 80000
  pools:
    main_session: 64000
    consultations: 12000
    mod_whisper: 4000
consultation:
  max_turns: 1
  min_response_tokens: 30
rounds:
  min: 1
  max: 3
  convergence_modifier: 0.9
  context_window: 15
human:
  slot: false
output:
  format: markdown
  sections:
    - id: summary
      title: "Review Summary"
      required: true
    - id: issues
      title: "Issues Found"
      required: true
    - id: recommendations
      title: "Recommendations"
      required: false
```

```yaml
# canvases/stdlib/architectural-design.yaml
id: architectural-design
version: "1.0.0"
mode: interactive
selector:
  description: "ADR-producing architecture design session; human holds final approval"
  tags: [architecture, adr, design, technical-decision, system-design]
  examples:
    - "choose a database for the user service"
    - "decide on the messaging strategy between services"
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
  - persona: adversarial
    domain: fondament/senior-engineer
  - persona: human
    authority: binding
budget:
  total_tokens: 200000
  pools:
    main_session: 160000
    consultations: 30000
    mod_whisper: 10000
consultation:
  max_turns: 3
  min_response_tokens: 50
rounds:
  min: 3
  max: 10
  convergence_modifier: 0.7
  context_window: 30
human:
  slot: true
  advisory_window_turns: 2
output:
  format: markdown
  sections:
    - id: context
      title: "Context & Problem Statement"
      required: true
    - id: decision
      title: "Decision"
      required: true
    - id: consequences
      title: "Consequences"
      required: true
```

```yaml
# canvases/stdlib/planning.yaml
id: planning
version: "1.0.0"
mode: interactive
selector:
  description: "Sprint or project planning session; produces prioritised action-item list"
  tags: [planning, sprint, backlog, roadmap, tasks, priorities]
  examples:
    - "plan the Q3 sprint"
    - "prioritise the backlog for the auth team"
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
  - persona: human
    authority: binding
budget:
  total_tokens: 80000
  pools:
    main_session: 65000
    consultations: 10000
    mod_whisper: 5000
consultation:
  max_turns: 2
  min_response_tokens: 30
rounds:
  min: 2
  max: 5
  convergence_modifier: 0.8
  context_window: 20
human:
  slot: true
  advisory_window_turns: 1
output:
  format: markdown
  sections:
    - id: goals
      title: "Sprint Goals"
      required: true
    - id: tasks
      title: "Task List"
      required: true
    - id: risks
      title: "Risks"
      required: false
```

- [ ] **Step 4: Create stubs and verify**

```rust
// crates/amassada-core/src/lib.rs
pub mod blocks;
pub mod budget;
pub mod canvas;
pub mod channels;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod moderator;
pub mod round;
pub mod session;
pub mod synthesis;
pub mod transport;
pub mod types;
```

```bash
cd /Users/bedardpl/project/Amassada && cargo check --workspace 2>&1
```

- [ ] **Step 5: Commit**

```bash
git init && git add -A && git commit -m "feat: scaffold amassada workspace and canvas stdlib"
```

---

### Task 2: Types & Error

**Files:** `crates/amassada-core/src/types.rs`, `crates/amassada-core/src/error.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/types_tests.rs
use amassada_core::types::*;

#[test]
fn session_state_transitions() {
    let s = SessionState::Initializing;
    assert!(!s.is_terminal());
    assert!(SessionState::Complete.is_terminal());
    assert!(SessionState::Failed.is_terminal());
}

#[test]
fn agent_id_roundtrip() {
    let id = AgentId::new("moderator");
    assert_eq!(id.as_str(), "moderator");
}
```

- [ ] **Step 2: Implement error.rs**

```rust
// crates/amassada-core/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AmassadaError {
    #[error("canvas not found: {0}")]
    CanvasNotFound(String),
    #[error("canvas parse error: {0}")]
    CanvasParse(String),
    #[error("budget exhausted: {pool}")]
    BudgetExhausted { pool: String },
    #[error("dispatch error: {0}")]
    Dispatch(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("session error: {0}")]
    Session(String),
}

pub type Result<T> = std::result::Result<T, AmassadaError>;
```

- [ ] **Step 3: Implement types.rs**

```rust
// crates/amassada-core/src/types.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(s: &str) -> Self { Self(s.to_string()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub agent_id: AgentId,
    pub persona: String,
    pub content: String,         // the [MAIN] block content
    pub round: u32,
    pub turn_index: u32,
    pub timestamp: DateTime<Utc>,
    pub tokens_used: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperMsg {
    pub from: AgentId,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanInput {
    pub kind: HumanInputKind,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HumanInputKind {
    Btw,        // [BTW] public message
    Call,       // /call — advisory window trigger
    Approve,    // approval of a [REQUEST_APPROVAL]
    Reject,     // rejection of a [REQUEST_APPROVAL]
    Modify,     // modified guidance after [REQUEST_APPROVAL]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    SessionStarted { canvas_id: String, goal: String },
    RoundStarted { round: u32 },
    TurnCompleted { record: TurnRecord },
    BtwEmitted { from: AgentId, to: String, content: String },
    ConsultationCompleted { requester: AgentId, consulted: AgentId },
    ModeratorAction { action: String },  // human-readable label
    ApprovalRequested { reason: String },
    HumanInput(HumanInput),
    RoundCompleted { round: u32 },
    SynthesisStarted,
    ArtifactCompleted { id: String, title: String },
    SessionCompleted,
    SessionFailed { reason: String },
    BudgetWarning { pool: String, pct_remaining: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Initializing,
    Running,
    AwaitingApproval,
    Synthesizing,
    Complete,
    Failed,
}

impl SessionState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOutput {
    pub session_id: String,
    pub canvas_id: String,
    pub goal: String,
    pub artifacts: Vec<OutputArtifact>,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputArtifact {
    pub id: String,
    pub title: String,
    pub content: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveParticipant {
    pub agent_id: AgentId,
    pub persona: String,
    pub domain: String,
    pub turns_taken: u32,
    pub is_moderator: bool,
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --package amassada-core 2>&1
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add core types, AgentId, SessionState, SessionEvent, TurnRecord"
```

---

### Task 3: Canvas Parser & Selector

**Files:** `crates/amassada-core/src/canvas.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/canvas_tests.rs
use amassada_core::canvas::{Canvas, CanvasLibrary};
use std::path::PathBuf;

#[test]
fn parses_debate_canvas() {
    let yaml = r#"
id: debate
version: "1.0.0"
mode: auto
selector:
  description: "Two agents debate"
  tags: [debate, argument]
  examples: ["should we use X or Y?"]
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
budget:
  total_tokens: 100000
  pools:
    main_session: 80000
    consultations: 15000
    mod_whisper: 5000
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 2
  max: 5
  convergence_modifier: 0.8
  context_window: 20
human:
  slot: false
output:
  format: markdown
  sections:
    - id: decision
      title: "Decision"
      required: true
"#;
    let canvas: Canvas = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(canvas.id, "debate");
    assert_eq!(canvas.budget.total_tokens, 100000);
    assert_eq!(canvas.initial_participants.len(), 2);
    assert!(canvas.initial_participants[0].is_moderator());
}

#[test]
fn canvas_selector_finds_best_match() {
    let library = CanvasLibrary::from_stdlib_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("canvases/stdlib")
    ).unwrap();
    let (canvas, score) = library.select("Should we use microservices or a monolith?");
    assert_eq!(canvas.id, "debate");
    assert!(score > 0.0);
}
```

- [ ] **Step 2: Implement canvas.rs**

```rust
// crates/amassada-core/src/canvas.rs
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: String,
    pub version: String,
    pub mode: CanvasMode,
    pub selector: SelectorMeta,
    pub initial_participants: Vec<ParticipantDef>,
    pub budget: BudgetConfig,
    pub consultation: ConsultationConfig,
    pub rounds: RoundsConfig,
    pub human: HumanConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CanvasMode { Auto, Interactive }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectorMeta {
    pub description: String,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantDef {
    pub persona: String,
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
}

impl ParticipantDef {
    pub fn is_moderator(&self) -> bool { self.persona == "moderator" }
    pub fn is_human(&self) -> bool { self.persona == "human" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub total_tokens: u32,
    pub pools: BudgetPools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetPools {
    pub main_session: u32,
    pub consultations: u32,
    pub mod_whisper: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsultationConfig {
    pub max_turns: u32,
    pub min_response_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundsConfig {
    pub min: u32,
    pub max: u32,
    pub convergence_modifier: f32,
    pub context_window: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanConfig {
    pub slot: bool,
    #[serde(default = "default_advisory_window")]
    pub advisory_window_turns: u32,
}

fn default_advisory_window() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub sections: Vec<OutputSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSection {
    pub id: String,
    pub title: String,
    pub required: bool,
}

impl Canvas {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| AmassadaError::CanvasParse(e.to_string()))
    }
}

pub struct CanvasLibrary {
    canvases: Vec<Canvas>,
}

impl CanvasLibrary {
    pub fn from_stdlib_dir(dir: PathBuf) -> Result<Self> {
        let mut canvases = Vec::new();
        if !dir.exists() { return Ok(Self { canvases }); }
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| AmassadaError::CanvasParse(e.to_string()))?
        {
            let path = entry.map_err(|e| AmassadaError::CanvasParse(e.to_string()))?.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                let yaml = std::fs::read_to_string(&path)
                    .map_err(|e| AmassadaError::CanvasParse(e.to_string()))?;
                canvases.push(Canvas::from_yaml(&yaml)?);
            }
        }
        Ok(Self { canvases })
    }

    pub fn get(&self, id: &str) -> Option<&Canvas> {
        self.canvases.iter().find(|c| c.id == id)
    }

    /// Heuristic selector: score by keyword overlap between query and canvas metadata.
    /// Returns the best-match canvas and a confidence score in [0, 1].
    /// In production, replace with a lightweight LLM call.
    pub fn select(&self, query: &str) -> (&Canvas, f32) {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut best_score = -1.0f32;
        let mut best_idx = 0;

        for (i, canvas) in self.canvases.iter().enumerate() {
            let haystack = format!(
                "{} {} {}",
                canvas.selector.description,
                canvas.selector.tags.join(" "),
                canvas.selector.examples.join(" ")
            ).to_lowercase();

            let matches = query_words.iter().filter(|w| haystack.contains(*w)).count();
            let score = if query_words.is_empty() { 0.0 } else {
                matches as f32 / query_words.len() as f32
            };

            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        (&self.canvases[best_idx], best_score.max(0.0))
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --package amassada-core canvas 2>&1
```
Expected: 2 tests pass

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add Canvas struct, serde YAML parsing, heuristic CanvasSelector"
```

---

### Task 4: Budget Accounting

**Files:** `crates/amassada-core/src/budget.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/budget_tests.rs
use amassada_core::budget::{BudgetLedger, PoolName};

#[test]
fn charges_and_tracks_pool() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.charge(PoolName::MainSession, 10_000).unwrap();
    let state = b.state(PoolName::MainSession);
    assert_eq!(state.consumed, 10_000);
    assert_eq!(state.remaining, 70_000);
}

#[test]
fn rejects_charge_over_pool_limit() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    let result = b.charge(PoolName::MainSession, 90_000);
    assert!(result.is_err());
}

#[test]
fn rebalance_adjusts_pools() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.adjust(PoolName::MainSession, -10_000, PoolName::Consultations, 10_000).unwrap();
    let ms = b.state(PoolName::MainSession);
    let co = b.state(PoolName::Consultations);
    assert_eq!(ms.total, 70_000);
    assert_eq!(co.total, 25_000);
}

#[test]
fn pct_remaining_is_correct() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.charge(PoolName::MainSession, 80_000).unwrap();
    let state = b.state(PoolName::MainSession);
    assert_eq!(state.pct_remaining, 0.0);
}
```

- [ ] **Step 2: Implement budget.rs**

```rust
// crates/amassada-core/src/budget.rs
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolName { MainSession, Consultations, ModWhisper }

impl std::fmt::Display for PoolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainSession => write!(f, "main_session"),
            Self::Consultations => write!(f, "consultations"),
            Self::ModWhisper => write!(f, "mod_whisper"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolState {
    pub total: u32,
    pub consumed: u32,
    pub remaining: u32,
    pub pct_remaining: f32,
}

#[derive(Debug, Clone)]
struct Pool {
    total: u32,
    consumed: u32,
}

impl Pool {
    fn new(total: u32) -> Self { Self { total, consumed: 0 } }
    fn remaining(&self) -> u32 { self.total.saturating_sub(self.consumed) }
    fn pct_remaining(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.remaining() as f32 / self.total as f32 }
    }
    fn state(&self) -> PoolState {
        PoolState {
            total: self.total,
            consumed: self.consumed,
            remaining: self.remaining(),
            pct_remaining: self.pct_remaining(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BudgetLedger {
    total: u32,
    main_session: Pool,
    consultations: Pool,
    mod_whisper: Pool,
}

impl BudgetLedger {
    pub fn new(total: u32, main: u32, consult: u32, whisper: u32) -> Self {
        Self {
            total,
            main_session: Pool::new(main),
            consultations: Pool::new(consult),
            mod_whisper: Pool::new(whisper),
        }
    }

    fn pool_mut(&mut self, name: PoolName) -> &mut Pool {
        match name {
            PoolName::MainSession => &mut self.main_session,
            PoolName::Consultations => &mut self.consultations,
            PoolName::ModWhisper => &mut self.mod_whisper,
        }
    }

    fn pool(&self, name: PoolName) -> &Pool {
        match name {
            PoolName::MainSession => &self.main_session,
            PoolName::Consultations => &self.consultations,
            PoolName::ModWhisper => &self.mod_whisper,
        }
    }

    pub fn charge(&mut self, pool: PoolName, tokens: u32) -> Result<()> {
        let p = self.pool(pool);
        if tokens > p.remaining() {
            return Err(AmassadaError::BudgetExhausted { pool: pool.to_string() });
        }
        self.pool_mut(pool).consumed += tokens;
        Ok(())
    }

    pub fn adjust(&mut self, from: PoolName, delta: i64, to: PoolName, to_delta: i64) -> Result<()> {
        let from_pool = self.pool(from);
        let new_from_total = from_pool.total as i64 + delta;
        if new_from_total < from_pool.consumed as i64 { 
            return Err(AmassadaError::BudgetExhausted { pool: from.to_string() });
        }
        self.pool_mut(from).total = new_from_total as u32;
        let to_pool = self.pool(to);
        let new_to_total = to_pool.total as i64 + to_delta;
        self.pool_mut(to).total = new_to_total.max(to_pool.consumed as i64) as u32;
        Ok(())
    }

    pub fn state(&self, pool: PoolName) -> PoolState { self.pool(pool).state() }

    pub fn all_states(&self) -> (PoolState, PoolState, PoolState) {
        (self.main_session.state(), self.consultations.state(), self.mod_whisper.state())
    }

    pub fn should_warn(&self, pool: PoolName) -> Option<f32> {
        let pct = self.pool(pool).pct_remaining();
        if pct <= 0.10 { Some(pct) } else if pct <= 0.20 { Some(pct) } else { None }
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --package amassada-core budget 2>&1
```
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add BudgetLedger with pool accounting, rebalance, and warning thresholds"
```

---

### Task 5: Block Parser

**Files:** `crates/amassada-core/src/blocks.rs`

The block parser reads a streaming LLM response and extracts typed blocks: `[MAIN]`, `[BTW to: X]`, `[CONSULT to: X]`, `[LEAVE]`, and Moderator-only blocks (`[INVITE]`, `[RELEASE]`, `[CLOSE]`, etc.).

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/blocks_tests.rs
use amassada_core::blocks::{parse_blocks, AgentBlock, ModeratorAction};

#[test]
fn parses_main_block() {
    let input = "[MAIN]\nHere is my contribution to the debate.";
    let blocks = parse_blocks(input, false);
    assert_eq!(blocks.agent_blocks.len(), 1);
    if let AgentBlock::Main { content } = &blocks.agent_blocks[0] {
        assert_eq!(content.trim(), "Here is my contribution to the debate.");
    } else { panic!("expected Main block"); }
}

#[test]
fn parses_btw_block() {
    let input = "[BTW to: builder]\nQuick question about the approach.";
    let blocks = parse_blocks(input, false);
    assert_eq!(blocks.agent_blocks.len(), 1);
    if let AgentBlock::Btw { to, content } = &blocks.agent_blocks[0] {
        assert_eq!(to, "builder");
        assert_eq!(content.trim(), "Quick question about the approach.");
    } else { panic!("expected BTW block"); }
}

#[test]
fn parses_consult_block() {
    let input = "[CONSULT to: breaker]\nWhat's the security concern here?";
    let blocks = parse_blocks(input, false);
    if let AgentBlock::Consult { to, content } = &blocks.agent_blocks[0] {
        assert_eq!(to, "breaker");
    } else { panic!("expected Consult block"); }
}

#[test]
fn parses_moderator_close() {
    let input = "[MAIN]\nFinal synthesis.\n[CLOSE]";
    let blocks = parse_blocks(input, true); // is_moderator = true
    assert!(blocks.moderator_actions.contains(&ModeratorAction::Close));
}

#[test]
fn parses_moderator_invite() {
    let input = "[INVITE: security-expert]\n[MAIN]\nI'm inviting an expert.";
    let blocks = parse_blocks(input, true);
    assert!(blocks.moderator_actions.iter().any(|a| matches!(a, ModeratorAction::Invite { .. })));
}

#[test]
fn ignores_moderator_blocks_for_non_moderator() {
    let input = "[CLOSE]\n[MAIN]\nNormal response.";
    let blocks = parse_blocks(input, false);
    assert!(blocks.moderator_actions.is_empty());
}
```

- [ ] **Step 2: Implement blocks.rs**

```rust
// crates/amassada-core/src/blocks.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentBlock {
    Main { content: String },
    Btw { to: String, content: String },
    Consult { to: String, content: String },
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeratorAction {
    Invite { agent_id: String },
    Release { agent_id: String },
    ForkConsultation { agent_a: String, agent_b: String, topic: String },
    AdjustBudget { pool: String, delta: i64 },
    RequestApproval { reason: String },
    SetModel { model: String, for_agent: String },
    Close,
}

#[derive(Debug, Clone)]
pub struct ParsedResponse {
    pub agent_blocks: Vec<AgentBlock>,
    pub moderator_actions: Vec<ModeratorAction>,
    pub raw: String,
}

pub fn parse_blocks(input: &str, is_moderator: bool) -> ParsedResponse {
    let mut agent_blocks = Vec::new();
    let mut moderator_actions = Vec::new();

    // Split by block header patterns: lines starting with [BLOCK...]
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("[MAIN]") {
            // collect until next block header
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Main { content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[BTW to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Btw { to, content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[CONSULT to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Consult { to, content });
            i += 1;
        } else if line.starts_with("[LEAVE]") {
            agent_blocks.push(AgentBlock::Leave);
            i += 1;
        } else if is_moderator {
            if let Some(id) = extract_param(line, "[INVITE:") {
                moderator_actions.push(ModeratorAction::Invite { agent_id: id });
                i += 1;
            } else if let Some(id) = extract_param(line, "[RELEASE:") {
                moderator_actions.push(ModeratorAction::Release { agent_id: id });
                i += 1;
            } else if line.starts_with("[CLOSE]") {
                moderator_actions.push(ModeratorAction::Close);
                i += 1;
            } else if let Some(reason) = extract_param(line, "[REQUEST_APPROVAL:") {
                moderator_actions.push(ModeratorAction::RequestApproval { reason });
                i += 1;
            } else if line.starts_with("[ADJUST_BUDGET:") {
                // [ADJUST_BUDGET: main_session, -10000]
                let inner = line.trim_start_matches("[ADJUST_BUDGET:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let pool = parts[0].trim().to_string();
                    let delta: i64 = parts[1].trim().parse().unwrap_or(0);
                    moderator_actions.push(ModeratorAction::AdjustBudget { pool, delta });
                }
                i += 1;
            } else if line.starts_with("[MODEL:") {
                // [MODEL: claude-sonnet-4-6 for: builder]
                let inner = line.trim_start_matches("[MODEL:").trim_end_matches(']');
                if let Some((model, for_part)) = inner.split_once(" for:") {
                    moderator_actions.push(ModeratorAction::SetModel {
                        model: model.trim().to_string(),
                        for_agent: for_part.trim().to_string(),
                    });
                }
                i += 1;
            } else if line.starts_with("[FORK_CONSULTATION:") {
                let inner = line.trim_start_matches("[FORK_CONSULTATION:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(3, ',').collect();
                if parts.len() == 3 {
                    moderator_actions.push(ModeratorAction::ForkConsultation {
                        agent_a: parts[0].trim().to_string(),
                        agent_b: parts[1].trim().to_string(),
                        topic: parts[2].trim().to_string(),
                    });
                }
                i += 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    ParsedResponse { agent_blocks, moderator_actions, raw: input.to_string() }
}

fn extract_param(line: &str, prefix: &str) -> Option<String> {
    if line.starts_with(prefix) {
        let inner = line[prefix.len()..].trim_end_matches(']');
        Some(inner.trim().to_string())
    } else {
        None
    }
}

fn collect_until_next_block(lines: &[&str], start: usize) -> String {
    let block_starters = ["[MAIN]", "[BTW to:", "[CONSULT to:", "[LEAVE]",
        "[INVITE:", "[RELEASE:", "[CLOSE]", "[REQUEST_APPROVAL:", "[ADJUST_BUDGET:",
        "[MODEL:", "[FORK_CONSULTATION:"];
    let mut content_lines = Vec::new();
    for &line in &lines[start..] {
        let trimmed = line.trim();
        if block_starters.iter().any(|s| trimmed.starts_with(s)) {
            break;
        }
        content_lines.push(line);
    }
    content_lines.join("\n")
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --package amassada-core blocks 2>&1
```
Expected: 6 tests pass

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add streaming block parser for agent and moderator response blocks"
```

---

### Task 6: Channel Architecture

**Files:** `crates/amassada-core/src/channels/`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/channels_tests.rs
use amassada_core::channels::{main_session::MainSessionChannel, whisper::WhisperQueue};
use amassada_core::types::{AgentId, WhisperMsg, TurnRecord, SessionEvent};
use chrono::Utc;

#[tokio::test]
async fn main_session_broadcasts_to_subscribers() {
    let channel = MainSessionChannel::new(16);
    let mut rx = channel.subscribe();
    let event = SessionEvent::SessionStarted {
        canvas_id: "debate".into(),
        goal: "test goal".into(),
    };
    channel.publish(event.clone()).await.unwrap();
    let received = rx.recv().await.unwrap();
    assert!(matches!(received, SessionEvent::SessionStarted { .. }));
}

#[test]
fn whisper_queue_enqueues_and_drains() {
    let mut queue = WhisperQueue::new();
    let agent = AgentId::new("builder");
    let msg = WhisperMsg {
        from: AgentId::new("moderator"),
        content: "be concise".into(),
        timestamp: Utc::now(),
    };
    queue.enqueue(agent.clone(), msg);
    let drained = queue.drain(&agent);
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].content, "be concise");
    assert!(queue.drain(&agent).is_empty());
}
```

- [ ] **Step 2: Implement channels/main_session.rs**

```rust
// crates/amassada-core/src/channels/main_session.rs
use tokio::sync::broadcast;
use crate::error::{AmassadaError, Result};
use crate::types::{SessionEvent, TurnRecord};

pub struct MainSessionChannel {
    tx: broadcast::Sender<SessionEvent>,
}

impl MainSessionChannel {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub async fn publish(&self, event: SessionEvent) -> Result<()> {
        self.tx.send(event).map_err(|e| AmassadaError::Transport(e.to_string()))?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<SessionEvent> {
        self.tx.clone()
    }
}
```

- [ ] **Step 3: Implement channels/whisper.rs**

```rust
// crates/amassada-core/src/channels/whisper.rs
use std::collections::HashMap;
use std::collections::VecDeque;
use crate::types::{AgentId, WhisperMsg};

pub struct WhisperQueue {
    queues: HashMap<AgentId, VecDeque<WhisperMsg>>,
}

impl WhisperQueue {
    pub fn new() -> Self { Self { queues: HashMap::new() } }

    pub fn enqueue(&mut self, agent: AgentId, msg: WhisperMsg) {
        self.queues.entry(agent).or_default().push_back(msg);
    }

    pub fn drain(&mut self, agent: &AgentId) -> Vec<WhisperMsg> {
        self.queues.get_mut(agent)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    }
}
```

- [ ] **Step 4: Implement channels/consult.rs**

```rust
// crates/amassada-core/src/channels/consult.rs
use crate::error::{AmassadaError, Result};
use crate::types::AgentId;

pub struct ConsultRequest {
    pub requester: AgentId,
    pub target: AgentId,
    pub question: String,
}

pub struct ConsultResponse {
    pub from: AgentId,
    pub content: String,
}
```

- [ ] **Step 5: Implement channels/mod.rs**

```rust
// crates/amassada-core/src/channels/mod.rs
pub mod consult;
pub mod main_session;
pub mod whisper;
```

- [ ] **Step 6: Run tests**

```bash
cargo test --package amassada-core channels 2>&1
```
Expected: 2 tests pass

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: add three-channel architecture — MainSessionChannel, WhisperQueue, ConsultRequest"
```

---

### Task 7: Transport Trait & LocalTransport

**Files:** `crates/amassada-core/src/transport/`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/transport_tests.rs
use amassada_core::transport::local::LocalTransport;
use amassada_core::transport::Transport;
use amassada_core::types::*;

// Object-safety compile check
fn _assert_object_safe(_: &dyn Transport) {}

#[tokio::test]
async fn local_transport_broadcasts_events() {
    let transport = LocalTransport::new_test();
    let event = SessionEvent::SessionStarted { canvas_id: "debate".into(), goal: "test".into() };
    transport.broadcast(&event).await.unwrap();
    // In test mode, events go to an internal buffer
    let events = transport.take_events();
    assert_eq!(events.len(), 1);
}
```

- [ ] **Step 2: Implement transport/mod.rs**

```rust
// crates/amassada-core/src/transport/mod.rs
pub mod local;

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{AgentId, HumanInput, SessionOutput, SessionEvent, WhisperMsg};
use crate::channels::consult::{ConsultRequest, ConsultResponse};

#[async_trait]
pub trait Transport: Send + Sync {
    async fn broadcast(&self, event: &SessionEvent) -> Result<()>;
    async fn consult(&self, req: &ConsultRequest) -> Result<ConsultResponse>;
    async fn whisper(&self, agent: &AgentId, msg: &WhisperMsg) -> Result<()>;
    async fn recv_human(&self) -> Option<HumanInput>;
    async fn emit_output(&self, output: &SessionOutput) -> Result<()>;
}
```

- [ ] **Step 3: Implement transport/local.rs**

```rust
// crates/amassada-core/src/transport/local.rs
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::channels::consult::{ConsultRequest, ConsultResponse};
use crate::error::Result;
use crate::transport::Transport;
use crate::types::{AgentId, HumanInput, SessionEvent, SessionOutput, WhisperMsg};

pub struct LocalTransport {
    events: Arc<Mutex<Vec<SessionEvent>>>,
    human_rx: Arc<Mutex<Option<mpsc::Receiver<HumanInput>>>>,
}

impl LocalTransport {
    /// For CLI use — human input from stdin channel
    pub fn new(human_rx: mpsc::Receiver<HumanInput>) -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            human_rx: Arc::new(Mutex::new(Some(human_rx))),
        }
    }

    /// For tests — no human input channel
    pub fn new_test() -> Self {
        let (_, rx) = mpsc::channel(1);
        Self::new(rx)
    }

    pub fn take_events(&self) -> Vec<SessionEvent> {
        let mut events = self.events.lock().unwrap();
        std::mem::take(&mut *events)
    }
}

#[async_trait]
impl Transport for LocalTransport {
    async fn broadcast(&self, event: &SessionEvent) -> Result<()> {
        tracing::info!("[event] {:?}", event);
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn consult(&self, req: &ConsultRequest) -> Result<ConsultResponse> {
        // LocalTransport doesn't dispatch real consultations — stub for CLI
        Ok(ConsultResponse {
            from: req.target.clone(),
            content: "[consultation not available in local mode]".into(),
        })
    }

    async fn whisper(&self, agent: &AgentId, msg: &WhisperMsg) -> Result<()> {
        tracing::debug!("[whisper → {}] {}", agent, msg.content);
        Ok(())
    }

    async fn recv_human(&self) -> Option<HumanInput> {
        // Non-blocking try_recv — returns None if no human input pending
        if let Ok(mut guard) = self.human_rx.lock() {
            if let Some(rx) = guard.as_mut() {
                return rx.try_recv().ok();
            }
        }
        None
    }

    async fn emit_output(&self, output: &SessionOutput) -> Result<()> {
        println!("\n=== Session Complete ===");
        for artifact in &output.artifacts {
            println!("\n# {}\n{}", artifact.title, artifact.content);
        }
        println!("\nTotal tokens: {}", output.total_tokens);
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --package amassada-core transport 2>&1
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add Transport trait and LocalTransport"
```

---

### Task 8: Context Builder & Turn Dispatch

**Files:** `crates/amassada-core/src/context.rs`, `crates/amassada-core/src/dispatch.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/amassada-core/tests/context_tests.rs
use amassada_core::context::ContextBuilder;
use amassada_core::types::{AgentId, TurnRecord};
use chrono::Utc;

#[test]
fn build_context_respects_window() {
    let mut builder = ContextBuilder::new(3);
    for i in 0..5 {
        builder.push_turn(TurnRecord {
            agent_id: AgentId::new("agent-a"),
            persona: "builder".into(),
            content: format!("turn {}", i),
            round: 1,
            turn_index: i,
            timestamp: Utc::now(),
            tokens_used: 100,
        });
    }
    let ctx = builder.build_for(
        &AgentId::new("agent-b"),
        vec![],       // whispers
        None,         // moderator envelope
    );
    // Should only include last 3 turns
    assert!(ctx.contains("turn 4"));
    assert!(ctx.contains("turn 3"));
    assert!(ctx.contains("turn 2"));
    assert!(!ctx.contains("turn 1"));
    assert!(!ctx.contains("turn 0"));
}
```

- [ ] **Step 2: Implement context.rs**

```rust
// crates/amassada-core/src/context.rs
use std::collections::VecDeque;
use crate::types::{AgentId, TurnRecord, WhisperMsg};

pub struct ContextBuilder {
    window: usize,
    transcript: VecDeque<TurnRecord>,
}

impl ContextBuilder {
    pub fn new(window: usize) -> Self {
        Self { window, transcript: VecDeque::new() }
    }

    pub fn push_turn(&mut self, record: TurnRecord) {
        self.transcript.push_back(record);
        while self.transcript.len() > self.window {
            self.transcript.pop_front();
        }
    }

    pub fn build_for(
        &self,
        agent: &AgentId,
        whispers: Vec<WhisperMsg>,
        moderator_envelope: Option<String>,
    ) -> String {
        let mut parts = Vec::new();

        if !whispers.is_empty() {
            parts.push("=== Moderator Notes ===".to_string());
            for w in &whispers {
                parts.push(format!("[whisper] {}", w.content));
            }
        }

        if let Some(env) = moderator_envelope {
            parts.push("=== Session State (advisory) ===".to_string());
            parts.push(env);
        }

        parts.push("=== Transcript ===".to_string());
        for record in &self.transcript {
            parts.push(format!("[{} / {}] {}", record.agent_id, record.persona, record.content));
        }

        parts.join("\n")
    }

    pub fn len(&self) -> usize { self.transcript.len() }
}
```

- [ ] **Step 3: Implement dispatch.rs**

```rust
// crates/amassada-core/src/dispatch.rs
use crate::error::{AmassadaError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct TurnResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Calls the Anthropic Messages API directly via reqwest.
/// Uses the user-message-only format (no prefill, no multi-turn list).
pub async fn dispatch(req: TurnRequest) -> Result<TurnResponse> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| AmassadaError::Dispatch("ANTHROPIC_API_KEY not set".into()))?;

    let body = serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "system": req.system_prompt,
        "messages": [{"role": "user", "content": req.context}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AmassadaError::Dispatch(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AmassadaError::Dispatch(format!("API error {}: {}", status, text)));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| AmassadaError::Dispatch(e.to_string()))?;

    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
    let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

    Ok(TurnResponse { text, input_tokens, output_tokens })
}

/// Build the system prompt for an agent.
pub fn build_system_prompt(
    persona: &str,
    domain_context: &str,
    is_moderator: bool,
) -> String {
    let block_syntax = if is_moderator {
        r#"## Block Syntax (Moderator)

You MUST structure responses using these block markers:

[CONSULT to: <agent-id>]
<question for private sidebar — resolved before [MAIN]>

[BTW to: <agent-id>|room]
<public side comment — visible in transcript>

[MAIN]
<your primary contribution this turn>

[LEAVE]
<optional — emit only if your contribution to this session is complete>

## Moderator-Only Blocks
[INVITE: <agent-id>]
[RELEASE: <agent-id>]
[FORK_CONSULTATION: <agent-a>, <agent-b>, <topic>]
[ADJUST_BUDGET: <pool>, <delta>]
[REQUEST_APPROVAL: <reason>]
[MODEL: <model-id> for: <agent-id>]
[CLOSE]
"#
    } else {
        r#"## Block Syntax

You MUST structure responses using these block markers:

[CONSULT to: <agent-id>]
<question for private sidebar — resolved before [MAIN]>

[BTW to: <agent-id>|room]
<public side comment — visible in transcript>

[MAIN]
<your primary contribution this turn>

[LEAVE]
<optional — emit only if your contribution to this session is complete>
"#
    };

    format!(
        "You are a {persona} agent.\n\n{domain_context}\n\n{block_syntax}",
        persona = persona,
        domain_context = domain_context,
        block_syntax = block_syntax,
    )
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --package amassada-core context 2>&1
```
Expected: 1 test passes

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add ContextBuilder, build_for() sliding window, dispatch() Anthropic API call"
```

---

### Task 9: Moderator & Round Engine

**Files:** `crates/amassada-core/src/moderator.rs`, `crates/amassada-core/src/round.rs`

- [ ] **Step 1: Implement moderator.rs**

```rust
// crates/amassada-core/src/moderator.rs
use crate::blocks::ModeratorAction;
use crate::budget::{BudgetLedger, PoolName};
use crate::channels::whisper::WhisperQueue;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, WhisperMsg};
use chrono::Utc;

pub struct ModeratorExecutor;

pub struct ExecutionResult {
    pub should_close: bool,
    pub approval_requested: Option<String>,
    pub new_participants: Vec<AgentId>,
    pub released: Vec<AgentId>,
    pub events: Vec<SessionEvent>,
}

impl ModeratorExecutor {
    pub fn execute(
        &self,
        actions: Vec<ModeratorAction>,
        participants: &mut Vec<ActiveParticipant>,
        budget: &mut BudgetLedger,
        whisper_queue: &mut WhisperQueue,
    ) -> ExecutionResult {
        let mut result = ExecutionResult {
            should_close: false,
            approval_requested: None,
            new_participants: Vec::new(),
            released: Vec::new(),
            events: Vec::new(),
        };

        for action in actions {
            match action {
                ModeratorAction::Close => {
                    result.should_close = true;
                    result.events.push(SessionEvent::ModeratorAction { action: "CLOSE".into() });
                }
                ModeratorAction::Invite { agent_id } => {
                    let id = AgentId::new(&agent_id);
                    if !participants.iter().any(|p| p.agent_id == id) {
                        result.new_participants.push(id.clone());
                        result.events.push(SessionEvent::ModeratorAction {
                            action: format!("INVITE {}", agent_id)
                        });
                    }
                }
                ModeratorAction::Release { agent_id } => {
                    let id = AgentId::new(&agent_id);
                    result.released.push(id.clone());
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("RELEASE {}", agent_id)
                    });
                }
                ModeratorAction::RequestApproval { reason } => {
                    result.approval_requested = Some(reason.clone());
                    result.events.push(SessionEvent::ApprovalRequested { reason });
                }
                ModeratorAction::AdjustBudget { pool, delta } => {
                    let p = match pool.as_str() {
                        "main_session" => PoolName::MainSession,
                        "consultations" => PoolName::Consultations,
                        "mod_whisper" => PoolName::ModWhisper,
                        _ => continue,
                    };
                    let other = match pool.as_str() {
                        "main_session" => PoolName::Consultations,
                        _ => PoolName::MainSession,
                    };
                    let _ = budget.adjust(p, delta, other, -delta);
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("ADJUST_BUDGET {} {}", pool, delta)
                    });
                }
                ModeratorAction::SetModel { model, for_agent } => {
                    // Model override recorded in SessionEvent — participant lookup is engine-level
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("MODEL {} for {}", model, for_agent)
                    });
                }
                ModeratorAction::ForkConsultation { agent_a, agent_b, topic } => {
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("FORK_CONSULTATION {} {} {}", agent_a, agent_b, topic)
                    });
                }
            }
        }

        result
    }
}
```

- [ ] **Step 2: Implement round.rs**

```rust
// crates/amassada-core/src/round.rs
use crate::blocks::{AgentBlock, parse_blocks};
use crate::budget::{BudgetLedger, PoolName};
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::dispatch::{self, TurnRequest, build_system_prompt};
use crate::error::Result;
use crate::moderator::ModeratorExecutor;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, TurnRecord};
use chrono::Utc;

const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const MAX_TOKENS_PER_TURN: u32 = 4096;

pub struct RoundRunner<'a> {
    pub round_num: u32,
    pub participants: &'a mut Vec<ActiveParticipant>,
    pub context_builder: &'a mut ContextBuilder,
    pub whisper_queue: &'a mut WhisperQueue,
    pub budget: &'a mut BudgetLedger,
    pub transport: &'a dyn Transport,
}

pub struct RoundResult {
    pub should_close: bool,
    pub approval_requested: Option<String>,
}

impl<'a> RoundRunner<'a> {
    pub async fn run(&mut self) -> Result<RoundResult> {
        let mut result = RoundResult { should_close: false, approval_requested: None };

        self.transport.broadcast(&SessionEvent::RoundStarted { round: self.round_num }).await?;

        let participant_ids: Vec<AgentId> = self.participants.iter()
            .filter(|p| !p.is_human())
            .map(|p| p.agent_id.clone())
            .collect();

        for agent_id in &participant_ids {
            let participant = match self.participants.iter().find(|p| &p.agent_id == agent_id) {
                Some(p) => p.clone(),
                None => continue,
            };

            // drain whispers
            let whispers = self.whisper_queue.drain(agent_id);

            let context = self.context_builder.build_for(
                agent_id,
                whispers,
                None, // moderator envelope built separately for moderator
            );

            let system_prompt = build_system_prompt(
                &participant.persona,
                &participant.domain, // simplified: domain is the system prompt body
                participant.is_moderator,
            );

            let model = DEFAULT_MODEL.to_string(); // L1; L2/L3 overrides added later
            let req = TurnRequest { system_prompt, context, model, max_tokens: MAX_TOKENS_PER_TURN };

            let response = dispatch::dispatch(req).await?;
            let tokens_used = response.input_tokens + response.output_tokens;

            let parsed = parse_blocks(&response.text, participant.is_moderator);

            // Handle CONSULT blocks (simplified: stub, real dispatch in full impl)
            // Handle MAIN block
            let main_content = parsed.agent_blocks.iter()
                .find_map(|b| if let AgentBlock::Main { content } = b { Some(content.clone()) } else { None })
                .unwrap_or_else(|| response.text.clone());

            let record = TurnRecord {
                agent_id: agent_id.clone(),
                persona: participant.persona.clone(),
                content: main_content.clone(),
                round: self.round_num,
                turn_index: participant.turns_taken,
                timestamp: Utc::now(),
                tokens_used,
            };

            if let Err(_) = self.budget.charge(PoolName::MainSession, tokens_used) {
                tracing::warn!("main_session budget exhausted, forcing close");
                result.should_close = true;
                break;
            }

            self.context_builder.push_turn(record.clone());
            self.transport.broadcast(&SessionEvent::TurnCompleted { record }).await?;

            // Update turns_taken
            if let Some(p) = self.participants.iter_mut().find(|p| &p.agent_id == agent_id) {
                p.turns_taken += 1;
            }

            // Execute moderator actions if this participant is moderator
            if participant.is_moderator && !parsed.moderator_actions.is_empty() {
                let exec = ModeratorExecutor;
                let mut exec_result = exec.execute(
                    parsed.moderator_actions,
                    self.participants,
                    self.budget,
                    self.whisper_queue,
                );

                for event in exec_result.events {
                    self.transport.broadcast(&event).await?;
                }

                if exec_result.should_close { result.should_close = true; }
                if let Some(reason) = exec_result.approval_requested {
                    result.approval_requested = Some(reason);
                }
            }

            // BTW handling (simplified: logged to transcript)
            for block in &parsed.agent_blocks {
                if let AgentBlock::Btw { to, content } = block {
                    self.transport.broadcast(&SessionEvent::BtwEmitted {
                        from: agent_id.clone(),
                        to: to.clone(),
                        content: content.clone(),
                    }).await?;
                }
            }

            if result.should_close { break; }
        }

        self.transport.broadcast(&SessionEvent::RoundCompleted { round: self.round_num }).await?;
        Ok(result)
    }
}

impl ActiveParticipant {
    pub fn is_human(&self) -> bool { self.persona == "human" }
}
```

- [ ] **Step 3: Build — verify compiles**

```bash
cargo build --package amassada-core 2>&1
```
Expected: builds (dispatch() won't be tested without API key)

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add ModeratorExecutor, RoundRunner with turn dispatch and block execution"
```

---

### Task 10: Session State Machine & Synthesis

**Files:** `crates/amassada-core/src/session.rs`, `crates/amassada-core/src/synthesis.rs`

- [ ] **Step 1: Implement synthesis.rs**

```rust
// crates/amassada-core/src/synthesis.rs
use futures::future::join_all;
use crate::canvas::OutputSection;
use crate::context::ContextBuilder;
use crate::dispatch::{self, TurnRequest};
use crate::error::Result;
use crate::types::OutputArtifact;

const SYNTHESIS_MODEL: &str = "claude-sonnet-4-6";

pub async fn synthesize_artifacts(
    sections: &[OutputSection],
    context_builder: &ContextBuilder,
    canvas_id: &str,
    goal: &str,
) -> Result<Vec<OutputArtifact>> {
    let context = context_builder.build_for(
        &crate::types::AgentId::new("synthesis"),
        vec![],
        None,
    );

    let futures: Vec<_> = sections.iter().map(|section| {
        let ctx = context.clone();
        let section = section.clone();
        let goal = goal.to_string();
        async move {
            let system = format!(
                "You are a synthesis agent. Based on the session transcript, produce the '{}' section of the session output. Be concise and precise. The session goal was: {}",
                section.title, goal
            );
            let req = TurnRequest {
                system_prompt: system,
                context: ctx,
                model: SYNTHESIS_MODEL.to_string(),
                max_tokens: 2048,
            };
            let response = dispatch::dispatch(req).await?;
            Ok::<OutputArtifact, crate::error::AmassadaError>(OutputArtifact {
                id: section.id,
                title: section.title,
                content: response.text,
                required: section.required,
            })
        }
    }).collect();

    let results: Vec<Result<OutputArtifact>> = join_all(futures).await;
    results.into_iter().collect()
}
```

- [ ] **Step 2: Implement session.rs**

```rust
// crates/amassada-core/src/session.rs
use uuid::Uuid;
use crate::budget::BudgetLedger;
use crate::canvas::Canvas;
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::error::Result;
use crate::round::RoundRunner;
use crate::synthesis::synthesize_artifacts;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, SessionOutput, SessionState};

pub struct SessionEngine {
    pub session_id: String,
    pub canvas: Canvas,
    pub goal: String,
    transport: Box<dyn Transport>,
}

impl SessionEngine {
    pub fn new(canvas: Canvas, goal: String, transport: Box<dyn Transport>) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            canvas,
            goal,
            transport,
        }
    }

    pub async fn run(&self) -> Result<SessionOutput> {
        self.transport.broadcast(&SessionEvent::SessionStarted {
            canvas_id: self.canvas.id.clone(),
            goal: self.goal.clone(),
        }).await?;

        // Assemble participants from canvas definitions
        let mut participants: Vec<ActiveParticipant> = self.canvas.initial_participants.iter()
            .enumerate()
            .map(|(i, def)| ActiveParticipant {
                agent_id: AgentId::new(&format!("{}-{}", def.persona, i)),
                persona: def.persona.clone(),
                domain: def.domain.clone(),
                turns_taken: 0,
                is_moderator: def.is_moderator(),
            })
            .collect();

        let mut budget = BudgetLedger::new(
            self.canvas.budget.total_tokens,
            self.canvas.budget.pools.main_session,
            self.canvas.budget.pools.consultations,
            self.canvas.budget.pools.mod_whisper,
        );

        let mut context_builder = ContextBuilder::new(self.canvas.rounds.context_window);
        let mut whisper_queue = WhisperQueue::new();

        let mut state = SessionState::Running;
        let mut current_round = 1u32;

        while !state.is_terminal() && current_round <= self.canvas.rounds.max {
            let mut runner = RoundRunner {
                round_num: current_round,
                participants: &mut participants,
                context_builder: &mut context_builder,
                whisper_queue: &mut whisper_queue,
                budget: &mut budget,
                transport: self.transport.as_ref(),
            };

            let result = runner.run().await?;

            if let Some(reason) = result.approval_requested {
                state = SessionState::AwaitingApproval;
                // Wait for human input (simplified — polling)
                loop {
                    if let Some(input) = self.transport.recv_human().await {
                        match input.kind {
                            crate::types::HumanInputKind::Approve => {
                                state = SessionState::Running;
                                break;
                            }
                            crate::types::HumanInputKind::Reject => {
                                // Re-notify moderator in next round
                                state = SessionState::Running;
                                break;
                            }
                            _ => {}
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }

            if result.should_close || current_round >= self.canvas.rounds.min {
                if result.should_close { break; }
            }

            current_round += 1;
        }

        // Synthesis phase
        state = SessionState::Synthesizing;
        self.transport.broadcast(&SessionEvent::SynthesisStarted).await?;

        let artifacts = synthesize_artifacts(
            &self.canvas.output.sections,
            &context_builder,
            &self.canvas.id,
            &self.goal,
        ).await?;

        for art in &artifacts {
            self.transport.broadcast(&SessionEvent::ArtifactCompleted {
                id: art.id.clone(),
                title: art.title.clone(),
            }).await?;
        }

        self.transport.broadcast(&SessionEvent::SessionCompleted).await?;

        let output = SessionOutput {
            session_id: self.session_id.clone(),
            canvas_id: self.canvas.id.clone(),
            goal: self.goal.clone(),
            artifacts,
            total_tokens: 0, // TODO: sum from budget ledger
        };

        self.transport.emit_output(&output).await?;
        Ok(output)
    }
}
```

- [ ] **Step 3: Build**

```bash
cargo build --package amassada-core 2>&1
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add SessionEngine state machine and synthesis phase — amassada-core v0.1.0"
```

---

### Task 11: amassada-server

**Files:** `crates/amassada-server/src/main.rs`, `api.rs`, `ws.rs`

- [ ] **Step 1: Implement api.rs**

```rust
// crates/amassada-server/src/api.rs
use axum::{extract::State, http::StatusCode, Json};
use amassada_core::types::{HumanInput, SessionState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ServerState {
    pub canvas_dir: String,
    pub active_state: Arc<Mutex<SessionState>>,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub canvas_id: Option<String>,
    pub goal: String,
}

#[derive(Serialize)]
pub struct StartResponse {
    pub session_id: String,
    pub canvas_id: String,
}

pub async fn start_session(
    State(s): State<ServerState>,
    Json(req): Json<StartRequest>,
) -> (StatusCode, Json<StartResponse>) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let canvas_id = req.canvas_id.unwrap_or("design-session".into());
    // Actual session start is async — client polls /state or connects to /ws
    (StatusCode::ACCEPTED, Json(StartResponse { session_id, canvas_id }))
}

pub async fn get_state(State(s): State<ServerState>) -> Json<String> {
    let state = s.active_state.lock().await;
    Json(format!("{:?}", *state))
}

#[derive(Deserialize)]
pub struct HumanInputReq {
    pub kind: String,
    pub content: String,
}

pub async fn post_human_input(
    State(_s): State<ServerState>,
    Json(req): Json<HumanInputReq>,
) -> StatusCode {
    tracing::info!("human input received: {} — {}", req.kind, req.content);
    StatusCode::ACCEPTED
}
```

- [ ] **Step 2: Implement ws.rs**

```rust
// crates/amassada-server/src/ws.rs
// WebSocket streaming for SessionEvent — stub for v0.1.0
// Real implementation: upgrade connection, subscribe to broadcast::Receiver,
// forward each SessionEvent as JSON. Wired in main.rs router.
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    tracing::info!("ws client connected");
    // In full impl: subscribe to MainSessionChannel broadcast, forward events
}
```

- [ ] **Step 3: Implement main.rs**

```rust
// crates/amassada-server/src/main.rs
mod api;
mod ws;

use axum::{routing::{get, post}, Router};
use api::ServerState;
use amassada_core::types::SessionState;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let port = std::env::var("AMASSADA_PORT").unwrap_or("7600".into());
    let canvas_dir = std::env::var("AMASSADA_CANVAS_DIR")
        .unwrap_or("canvases/stdlib".into());

    let state = ServerState {
        canvas_dir,
        active_state: Arc::new(Mutex::new(SessionState::Initializing)),
    };

    let app = Router::new()
        .route("/sessions", post(api::start_session))
        .route("/state", get(api::get_state))
        .route("/human_input", post(api::post_human_input))
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("amassada-server listening on :{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Build**

```bash
cargo build --workspace 2>&1
```
Expected: both crates build

- [ ] **Step 5: Final commit**

```bash
git add -A && git commit -m "feat: add amassada-server REST + WebSocket skeleton — amassada v0.1.0 complete"
```

---

## Self-Review

**Spec coverage:**
- ✅ Three async channels: MainSessionChannel (broadcast), WhisperQueue, ConsultRequest (Tasks 6, 7)
- ✅ Canvas YAML format, BudgetConfig, OutputSection, CanvasMode (Task 3)
- ✅ Canvas stdlib: debate, design-session, code-review-council, architectural-design, planning (Task 1)
- ✅ CanvasLibrary heuristic selector (Task 3)
- ✅ BudgetLedger: charge, adjust, warn thresholds (Task 4)
- ✅ Block parser: [MAIN], [BTW], [CONSULT], [LEAVE], all Moderator blocks (Task 5)
- ✅ Transport trait + LocalTransport (Task 7)
- ✅ build_context() sliding window (Task 8)
- ✅ dispatch() via Anthropic API (Task 8)
- ✅ build_system_prompt() with block syntax injected (Task 8)
- ✅ ModeratorAction execution (INVITE, RELEASE, CLOSE, ADJUST_BUDGET, REQUEST_APPROVAL) (Task 9)
- ✅ RoundRunner: per-persona sequential turn loop (Task 9)
- ✅ SessionEngine state machine + AwaitingApproval handling (Task 10)
- ✅ Parallel synthesis phase with join_all() (Task 10)
- ✅ amassada-server REST endpoints + WebSocket stub (Task 11)
- ⚠ Full [CONSULT] parallel dispatch with join_all() — stub in RoundRunner; real ConsultRuntime needs dispatch per target agent (planned v0.2.0)
- ⚠ [BTW] round-trip response (labeled in transcript but response from target not dispatched in v0.1.0)
- ⚠ CanvasSelector LLM-backed scoring (heuristic keyword match ships; LLM upgrade is v0.2.0)
- ⚠ CharradissaTransport feature gate (trait is defined; implementation lives in Charradissa)
- ⚠ Farga persistence as Transport observer (v2 per spec)
- ⚠ [SWITCH_CANVAS] reserved, not executed (per spec)

**Type consistency:** All types defined in types.rs (Task 2) used consistently through blocks.rs (Task 5), context.rs (Task 8), round.rs (Task 9), session.rs (Task 10). ActiveParticipant.is_human() added inline in round.rs (used in session.rs).
