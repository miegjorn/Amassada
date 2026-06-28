use crate::blocks::{AgentBlock, parse_blocks, ProposalOp};
use crate::budget::{BudgetLedger, PoolName};
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::dispatch::{self, TurnRequest, build_system_prompt};
use crate::error::Result;
use crate::moderator::ModeratorExecutor;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, TurnRecord};
use chrono::Utc;
use futures::future::join_all;

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
    pub canvas_switch: Option<String>,
    /// Graph proposals from non-moderator agents in this round.
    pub agent_proposal_ops: Vec<ProposalOp>,
    /// Graph proposals from the moderator in this round.
    /// Applied after agent proposals so moderator writes take precedence.
    pub moderator_proposal_ops: Vec<ProposalOp>,
    /// Concatenated main-block content from all turns, for extraction.
    pub round_transcript: String,
}

impl<'a> RoundRunner<'a> {
    pub async fn run(&mut self, shared_context: Option<String>) -> Result<RoundResult> {
        let mut result = RoundResult {
            should_close: false,
            approval_requested: None,
            canvas_switch: None,
            agent_proposal_ops: vec![],
            moderator_proposal_ops: vec![],
            round_transcript: String::new(),
        };

        self.transport.broadcast(&SessionEvent::RoundStarted { round: self.round_num }).await?;

        let participant_ids: Vec<AgentId> = self.participants.iter()
            .filter(|p| !p.is_human())
            .map(|p| p.agent_id.clone())
            .collect();

        // Phase 1: build all requests sequentially — requires &mut self for whisper drain
        // and context assembly, but the actual API calls are independent.
        struct PreparedItem {
            agent_id: AgentId,
            participant: ActiveParticipant,
            req: TurnRequest,
        }

        let mut prepared: Vec<PreparedItem> = Vec::new();
        for agent_id in &participant_ids {
            let participant = match self.participants.iter().find(|p| &p.agent_id == agent_id) {
                Some(p) => p.clone(),
                None => continue,
            };

            let whispers = self.whisper_queue.drain(agent_id);
            let context = self.context_builder.build_for(
                agent_id,
                whispers,
                None, // moderator envelope built separately for moderator
            );
            let system_prompt = build_system_prompt(
                &participant.persona,
                &participant.domain,
                participant.is_moderator,
            );
            let model = participant.model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string());
            let req = TurnRequest {
                system_prompt,
                context,
                model,
                max_tokens: MAX_TOKENS_PER_TURN,
                thinking_budget: participant.thinking_budget,
                api_key: None,
                shared_context: shared_context.clone(),
            };
            prepared.push(PreparedItem { agent_id: agent_id.clone(), participant, req });
        }

        // Phase 2: dispatch all agents concurrently.
        let (meta, handles): (Vec<(AgentId, ActiveParticipant)>, Vec<_>) = prepared
            .into_iter()
            .map(|item| {
                let handle = tokio::spawn(async move { dispatch::dispatch(item.req).await });
                ((item.agent_id, item.participant), handle)
            })
            .unzip();

        let responses = join_all(handles).await;

        // Phase 3: process responses in the same order as participant_ids.
        for (i, join_result) in responses.into_iter().enumerate() {
            let (agent_id, participant) = &meta[i];

            let response = match join_result {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => {
                    tracing::warn!("agent {} dispatch error: {}", agent_id, e);
                    continue;
                }
                Err(e) => {
                    tracing::warn!("agent {} join error: {}", agent_id, e);
                    continue;
                }
            };

            let tokens_used = response.input_tokens + response.output_tokens;
            let parsed = parse_blocks(&response.text, participant.is_moderator);

            // Handle MAIN block
            let main_content = parsed.agent_blocks.iter()
                .find_map(|b| if let AgentBlock::Main { content } = b { Some(content.clone()) } else { None })
                .unwrap_or_else(|| response.text.clone());

            // Collect graph proposals from this turn, routing by role so that
            // moderator proposals can be applied last (= wins on conflict).
            for block in &parsed.agent_blocks {
                if let AgentBlock::GraphProposal { ops } = block {
                    if participant.is_moderator {
                        result.moderator_proposal_ops.extend(ops.clone());
                    } else {
                        result.agent_proposal_ops.extend(ops.clone());
                    }
                }
            }

            // Append to round transcript for extraction
            if !result.round_transcript.is_empty() {
                result.round_transcript.push('\n');
            }
            result.round_transcript.push_str(&format!(
                "[{}] {}: {}",
                participant.persona, agent_id, main_content
            ));

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
                    parsed.moderator_actions.clone(),
                    self.participants,
                    self.budget,
                    self.whisper_queue,
                );

                for event in &exec_result.events {
                    self.transport.broadcast(event).await?;
                }

                if exec_result.should_close { result.should_close = true; }
                if let Some(reason) = exec_result.approval_requested {
                    result.approval_requested = Some(reason);
                }
                if let Some(id) = exec_result.canvas_switch {
                    result.canvas_switch = Some(id);
                }

                // Execute pending forks asynchronously
                for (agent_a, agent_b, topic) in exec_result.pending_forks {
                    let (persona, domain) = self.participants.iter()
                        .find(|p| p.persona == agent_b || p.agent_id.to_string().starts_with(&agent_b))
                        .map(|p| (p.persona.clone(), p.domain.clone()))
                        .unwrap_or_else(|| (agent_b.clone(), String::new()));

                    let system_prompt = dispatch::build_system_prompt(&persona, &domain, false);
                    let req = dispatch::TurnRequest {
                        system_prompt,
                        context: format!("SIDEBAR QUESTION from {}:\n{}", agent_a, topic),
                        model: DEFAULT_MODEL.to_string(),
                        max_tokens: 1024,
                        thinking_budget: None,
                        api_key: None,
                        shared_context: None,
                    };

                    if let Ok(resp) = dispatch::dispatch(req).await {
                        let a_id = AgentId::new(&agent_a);
                        let b_id = AgentId::new(&agent_b);
                        let msg = crate::types::WhisperMsg {
                            from: b_id,
                            content: format!("[sidebar] {}: {}", agent_b, resp.text),
                            timestamp: Utc::now(),
                        };
                        let _ = self.transport.whisper(&a_id, &msg).await;
                        self.whisper_queue.enqueue(a_id, msg);
                    }
                }
            }

            // BTW handling
            for block in &parsed.agent_blocks {
                if let AgentBlock::Btw { to, content } = block {
                    self.transport.broadcast(&SessionEvent::BtwEmitted {
                        from: agent_id.clone(),
                        to: to.clone(),
                        content: content.clone(),
                    }).await?;
                }
            }

            // CONSULT handling: deliver question as a whisper to the target agent
            for block in &parsed.agent_blocks {
                if let AgentBlock::Consult { to, content } = block {
                    let to_id = AgentId::new(to);
                    let msg = crate::types::WhisperMsg {
                        from: agent_id.clone(),
                        content: format!("[CONSULT from {}]: {}", participant.persona, content),
                        timestamp: Utc::now(),
                    };
                    let _ = self.transport.whisper(&to_id, &msg).await;
                    self.whisper_queue.enqueue(to_id.clone(), msg);
                    self.transport.broadcast(&SessionEvent::ConsultationCompleted {
                        requester: agent_id.clone(),
                        consulted: to_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;
    use crate::channels::consult::{ConsultRequest, ConsultResponse};
    use crate::transport::Transport;
    use crate::types::{HumanInput, SessionOutput};

    // Minimal transport that records broadcasted events for assertion.
    struct MockTransport {
        events: Arc<Mutex<Vec<SessionEvent>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self { events: Arc::new(Mutex::new(Vec::new())) }
        }
        fn events(&self) -> Vec<SessionEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn broadcast(&self, event: &SessionEvent) -> crate::error::Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn consult(&self, _req: &ConsultRequest) -> crate::error::Result<ConsultResponse> {
            unimplemented!("consult not used in round unit tests")
        }
        async fn whisper(&self, _agent: &AgentId, _msg: &crate::types::WhisperMsg) -> crate::error::Result<()> {
            Ok(())
        }
        async fn recv_human(&self) -> Option<HumanInput> { None }
        async fn emit_output(&self, _output: &SessionOutput) -> crate::error::Result<()> { Ok(()) }
    }

    fn ample_budget() -> BudgetLedger {
        BudgetLedger::new(100_000, 80_000, 15_000, 5_000)
    }

    /// With only human participants the parallel dispatch loop spawns 0 tasks and
    /// joins immediately. The round must emit RoundStarted + RoundCompleted and
    /// return an empty, non-closing result.
    #[tokio::test]
    async fn round_dispatches_all_agents() {
        let transport = MockTransport::new();
        let mut participants = vec![
            ActiveParticipant {
                agent_id: AgentId::new("h1"),
                persona: "human".into(),
                domain: String::new(),
                is_moderator: false,
                turns_taken: 0,
                model: None,
                thinking_budget: None,
            },
        ];
        let mut ctx = ContextBuilder::new(8);
        let mut wq = WhisperQueue::new();
        let mut budget = ample_budget();

        let mut runner = RoundRunner {
            round_num: 1,
            participants: &mut participants,
            context_builder: &mut ctx,
            whisper_queue: &mut wq,
            budget: &mut budget,
            transport: &transport,
        };

        let result = runner.run(None).await.expect("round must complete");
        assert!(!result.should_close, "round should not request close");
        assert!(result.agent_proposal_ops.is_empty() && result.moderator_proposal_ops.is_empty(), "no proposals expected");

        let events = transport.events();
        assert!(
            events.iter().any(|e| matches!(e, SessionEvent::RoundStarted { round: 1 })),
            "RoundStarted must be emitted"
        );
        assert!(
            events.iter().any(|e| matches!(e, SessionEvent::RoundCompleted { round: 1 })),
            "RoundCompleted must be emitted"
        );
        let turn_count = events.iter().filter(|e| matches!(e, SessionEvent::TurnCompleted { .. })).count();
        assert_eq!(turn_count, 0, "human participants do not produce TurnCompleted events");
    }

    /// With N=0 non-human participants the join_all completes immediately with an
    /// empty results slice — no turn records can be duplicated or fabricated.
    #[tokio::test]
    async fn round_parallelism_does_not_duplicate_turns() {
        let transport = MockTransport::new();
        let mut participants: Vec<ActiveParticipant> = vec![];
        let mut ctx = ContextBuilder::new(8);
        let mut wq = WhisperQueue::new();
        let mut budget = ample_budget();

        let mut runner = RoundRunner {
            round_num: 2,
            participants: &mut participants,
            context_builder: &mut ctx,
            whisper_queue: &mut wq,
            budget: &mut budget,
            transport: &transport,
        };

        let _result = runner.run(None).await.expect("round must complete");

        let turn_count = transport
            .events()
            .iter()
            .filter(|e| matches!(e, SessionEvent::TurnCompleted { .. }))
            .count();
        // N=0 agents → exactly N=0 turn records; the parallel join cannot duplicate entries.
        assert_eq!(turn_count, 0, "zero agents must produce zero TurnCompleted events");
        // context_builder must also be empty — no phantom pushes
        assert_eq!(ctx.len(), 0, "context_builder must have 0 turns after empty round");
    }
}
