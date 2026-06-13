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

        let _ = agent; // agent param reserved for future filtering
        parts.join("\n")
    }

    pub fn len(&self) -> usize { self.transcript.len() }
}
