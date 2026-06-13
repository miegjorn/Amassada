use tokio::sync::broadcast;
use crate::error::{AmassadaError, Result};
use crate::types::SessionEvent;

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
