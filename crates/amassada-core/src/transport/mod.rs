pub mod local;

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{AgentId, HumanInput, SessionOutput, SessionEvent, WhisperMsg};
use crate::channels::consult::{ConsultRequest, ConsultResponse};

#[async_trait]
pub trait Transport: Send + Sync {
    async fn broadcast(&self, event: &SessionEvent) -> Result<()>;
    /// Dispatch a single consultation question to the target agent and return
    /// the answer. This is a one-shot synchronous exchange — at most one question
    /// and one follow-up, never a multi-turn sub-conversation. Implementations
    /// MUST NOT initiate further consultation turns (no recursive consult calls).
    /// The caller whispers the answer to the requester's queue after this returns.
    async fn consult(&self, req: &ConsultRequest) -> Result<ConsultResponse>;
    async fn whisper(&self, agent: &AgentId, msg: &WhisperMsg) -> Result<()>;
    async fn recv_human(&self) -> Option<HumanInput>;
    async fn emit_output(&self, output: &SessionOutput) -> Result<()>;
}
