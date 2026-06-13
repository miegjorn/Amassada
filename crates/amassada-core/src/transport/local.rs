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
        tracing::debug!("[whisper -> {}] {}", agent, msg.content);
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
