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
