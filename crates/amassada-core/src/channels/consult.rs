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
