use chrono::{DateTime, Utc};
use fondament_core::types::StructuredReasoning;
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
    pub model: Option<String>,
    pub structured_reasoning: Option<StructuredReasoning>,
    pub is_aporia: bool,
    /// When set, this participant's turn is dispatched to an external agent endpoint
    /// (POST {endpoint}/turn) instead of calling Anthropic directly. Carried from
    /// `ParticipantDef::endpoint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// When true, context is built via ContextBuilder::build_for_sealed: only moderator
    /// whispers are visible, no session transcript, no shared graph context.
    #[serde(default)]
    pub context_seal: bool,
    #[serde(skip)]
    pub collected_parts: Vec<fondament_core::types::ComposedPart>,
}
