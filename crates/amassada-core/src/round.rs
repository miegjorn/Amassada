use crate::blocks::{AgentBlock, parse_blocks};
use crate::budget::{BudgetLedger, PoolName};
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::dispatch::{self, TurnRequest, build_system_prompt};
use crate::error::Result;
use crate::moderator::ModeratorExecutor;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, TurnRecord};
use chrono::Utc;

const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const MAX_TOKENS_PER_TURN: u32 = 4096;

pub struct RoundRunner<'a> {
    pub round_num: u32,
    pub participants: &'a mut Vec<ActiveParticipant>,
    pub context_builder: &'a mut ContextBuilder,
    pub whisper_queue: &'a mut WhisperQueue,
    pub budget: &'a mut BudgetLedger,
    pub transport: &'a dyn Transport,
}

pub struct RoundResult {
    pub should_close: bool,
    pub approval_requested: Option<String>,
}

impl<'a> RoundRunner<'a> {
    pub async fn run(&mut self) -> Result<RoundResult> {
        let mut result = RoundResult { should_close: false, approval_requested: None };

        self.transport.broadcast(&SessionEvent::RoundStarted { round: self.round_num }).await?;

        let participant_ids: Vec<AgentId> = self.participants.iter()
            .filter(|p| !p.is_human())
            .map(|p| p.agent_id.clone())
            .collect();

        for agent_id in &participant_ids {
            let participant = match self.participants.iter().find(|p| &p.agent_id == agent_id) {
                Some(p) => p.clone(),
                None => continue,
            };

            // drain whispers
            let whispers = self.whisper_queue.drain(agent_id);

            let context = self.context_builder.build_for(
                agent_id,
                whispers,
                None, // moderator envelope built separately for moderator
            );

            let system_prompt = build_system_prompt(
                &participant.persona,
                &participant.domain, // simplified: domain is the system prompt body
                participant.is_moderator,
            );

            let model = participant.model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string());
            let req = TurnRequest { system_prompt, context, model, max_tokens: MAX_TOKENS_PER_TURN, thinking_budget: participant.thinking_budget };

            let response = dispatch::dispatch(req).await?;
            let tokens_used = response.input_tokens + response.output_tokens;

            let parsed = parse_blocks(&response.text, participant.is_moderator);

            // Handle MAIN block
            let main_content = parsed.agent_blocks.iter()
                .find_map(|b| if let AgentBlock::Main { content } = b { Some(content.clone()) } else { None })
                .unwrap_or_else(|| response.text.clone());

            let record = TurnRecord {
                agent_id: agent_id.clone(),
                persona: participant.persona.clone(),
                content: main_content.clone(),
                round: self.round_num,
                turn_index: participant.turns_taken,
                timestamp: Utc::now(),
                tokens_used,
            };

            if let Err(_) = self.budget.charge(PoolName::MainSession, tokens_used) {
                tracing::warn!("main_session budget exhausted, forcing close");
                result.should_close = true;
                break;
            }

            self.context_builder.push_turn(record.clone());
            self.transport.broadcast(&SessionEvent::TurnCompleted { record }).await?;

            // Update turns_taken
            if let Some(p) = self.participants.iter_mut().find(|p| &p.agent_id == agent_id) {
                p.turns_taken += 1;
            }

            // Execute moderator actions if this participant is moderator
            if participant.is_moderator && !parsed.moderator_actions.is_empty() {
                let exec = ModeratorExecutor;
                let exec_result = exec.execute(
                    parsed.moderator_actions,
                    self.participants,
                    self.budget,
                    self.whisper_queue,
                );

                for event in exec_result.events {
                    self.transport.broadcast(&event).await?;
                }

                if exec_result.should_close { result.should_close = true; }
                if let Some(reason) = exec_result.approval_requested {
                    result.approval_requested = Some(reason);
                }
            }

            // BTW handling (simplified: logged to transcript)
            for block in &parsed.agent_blocks {
                if let AgentBlock::Btw { to, content } = block {
                    self.transport.broadcast(&SessionEvent::BtwEmitted {
                        from: agent_id.clone(),
                        to: to.clone(),
                        content: content.clone(),
                    }).await?;
                }
            }

            if result.should_close { break; }
        }

        self.transport.broadcast(&SessionEvent::RoundCompleted { round: self.round_num }).await?;
        Ok(result)
    }
}

impl ActiveParticipant {
    pub fn is_human(&self) -> bool { self.persona == "human" }
}
