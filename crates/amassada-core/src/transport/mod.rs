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
