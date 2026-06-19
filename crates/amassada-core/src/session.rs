use std::sync::Arc;
use uuid::Uuid;
use crate::budget::BudgetLedger;
use crate::canvas::Canvas;
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::error::Result;
use crate::round::RoundRunner;
use crate::synthesis::synthesize_artifacts;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, SessionOutput, SessionState};

pub struct SessionEngine {
    pub session_id: String,
    pub canvas: Canvas,
    pub goal: String,
    transport: Arc<dyn Transport>,
}

impl SessionEngine {
    pub fn new(canvas: Canvas, goal: String, transport: Arc<dyn Transport>) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            canvas,
            goal,
            transport,
        }
    }

    pub async fn run(&self) -> Result<SessionOutput> {
        self.transport.broadcast(&SessionEvent::SessionStarted {
            canvas_id: self.canvas.id.clone(),
            goal: self.goal.clone(),
        }).await?;

        // Assemble participants from canvas definitions
        let mut participants: Vec<ActiveParticipant> = self.canvas.initial_participants.iter()
            .enumerate()
            .map(|(i, def)| ActiveParticipant {
                agent_id: AgentId::new(&format!("{}-{}", def.persona, i)),
                persona: def.persona.clone(),
                domain: def.domain.clone(),
                turns_taken: 0,
                is_moderator: def.is_moderator(),
                model: def.model.clone(),
                thinking_budget: def.thinking_budget,
            })
            .collect();

        let mut budget = BudgetLedger::new(
            self.canvas.budget.total_tokens,
            self.canvas.budget.pools.main_session,
            self.canvas.budget.pools.consultations,
            self.canvas.budget.pools.mod_whisper,
        );

        let mut context_builder = ContextBuilder::new(self.canvas.rounds.context_window);
        let mut whisper_queue = WhisperQueue::new();

        let mut state = SessionState::Running;
        let mut current_round = 1u32;

        while !state.is_terminal() && current_round <= self.canvas.rounds.max {
            let mut runner = RoundRunner {
                round_num: current_round,
                participants: &mut participants,
                context_builder: &mut context_builder,
                whisper_queue: &mut whisper_queue,
                budget: &mut budget,
                transport: &*self.transport,
            };

            let result = runner.run().await?;

            if let Some(_reason) = result.approval_requested {
                state = SessionState::AwaitingApproval;
                // Wait for human input (simplified — polling)
                loop {
                    if let Some(input) = self.transport.recv_human().await {
                        match input.kind {
                            crate::types::HumanInputKind::Approve => {
                                state = SessionState::Running;
                                break;
                            }
                            crate::types::HumanInputKind::Reject => {
                                // Re-notify moderator in next round
                                state = SessionState::Running;
                                break;
                            }
                            _ => {}
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }

            if result.should_close || current_round >= self.canvas.rounds.min {
                if result.should_close { break; }
            }

            current_round += 1;
        }

        // Synthesis phase
        state = SessionState::Synthesizing;
        self.transport.broadcast(&SessionEvent::SynthesisStarted).await?;

        let artifacts = synthesize_artifacts(
            &self.canvas.output.sections,
            &context_builder,
            &self.canvas.id,
            &self.goal,
        ).await?;

        for art in &artifacts {
            self.transport.broadcast(&SessionEvent::ArtifactCompleted {
                id: art.id.clone(),
                title: art.title.clone(),
            }).await?;
        }

        self.transport.broadcast(&SessionEvent::SessionCompleted).await?;

        let output = SessionOutput {
            session_id: self.session_id.clone(),
            canvas_id: self.canvas.id.clone(),
            goal: self.goal.clone(),
            artifacts,
            total_tokens: 0, // TODO: sum from budget ledger
        };

        self.transport.emit_output(&output).await?;

        let _ = state; // state variable used for AwaitingApproval tracking
        Ok(output)
    }
}
