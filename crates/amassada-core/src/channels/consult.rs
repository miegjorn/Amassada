use crate::types::AgentId;

pub struct ConsultRequest {
    pub requester: AgentId,
    pub target: AgentId,
    pub question: String,
    /// System prompt to inject when dispatching the consulted agent.
    /// Built from the target participant's persona + domain by the caller.
    pub system_prompt: String,
    /// Model to use for the consultation dispatch.
    pub model: String,
}

pub struct ConsultResponse {
    pub from: AgentId,
    pub content: String,
    /// Token counts for budget charging at the call site.
    pub input_tokens: u32,
    pub output_tokens: u32,
}
