use crate::types::AgentId;

pub struct ConsultRequest {
    pub requester: AgentId,
    pub target: AgentId,
    pub question: String,
    /// System prompt for the target agent — supplied by the caller so the
    /// transport can dispatch the consultation without reaching back into
    /// session state.
    pub system_prompt: String,
    /// Model to use when dispatching to the target agent.
    pub model: String,
}

pub struct ConsultResponse {
    pub from: AgentId,
    pub content: String,
    /// Combined input + output token cost of the consultation dispatch.
    /// Callers charge this against `PoolName::MainSession`.
    pub tokens_used: u32,
}
