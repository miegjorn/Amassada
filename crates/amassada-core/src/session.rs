use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use crate::blocks::ProposalOp;
use crate::budget::BudgetLedger;
use crate::canvas::Canvas;
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::error::Result;
use crate::graph::{extract_delta, GraphDelta, NodeId, NodeType, NodeUpdate, SessionGraph};
use crate::round::RoundRunner;
use crate::synthesis::synthesize_artifacts;
use crate::transport::Transport;
use crate::types::{ActiveParticipant, AgentId, SessionEvent, SessionOutput, SessionState};

pub struct SessionEngine {
    pub session_id: String,
    pub canvas: Canvas,
    pub goal: String,
    pub graph: SessionGraph,
    transport: Arc<dyn Transport>,
    canvas_library: HashMap<String, Canvas>,
}

impl SessionEngine {
    pub fn new(canvas: Canvas, goal: String, transport: Arc<dyn Transport>) -> Self {
        let session_id = Uuid::new_v4().to_string();
        let graph = SessionGraph::new(&session_id);
        Self {
            session_id,
            canvas,
            goal,
            graph,
            transport,
            canvas_library: HashMap::new(),
        }
    }

    pub fn with_canvas_library(mut self, library: HashMap<String, Canvas>) -> Self {
        self.canvas_library = library;
        self
    }

    pub async fn run(&mut self) -> Result<SessionOutput> {
        // Use a local mutable canvas so hot-switch mid-session is possible
        let mut active_canvas = self.canvas.clone();

        self.transport.broadcast(&SessionEvent::SessionStarted {
            canvas_id: active_canvas.id.clone(),
            goal: self.goal.clone(),
        }).await?;

        // Assemble participants from canvas definitions
        let mut participants: Vec<ActiveParticipant> = active_canvas.initial_participants.iter()
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
            active_canvas.budget.total_tokens,
            active_canvas.budget.pools.main_session,
            active_canvas.budget.pools.consultations,
            active_canvas.budget.pools.mod_whisper,
        );

        let mut context_builder = ContextBuilder::new(active_canvas.rounds.context_window);
        let mut whisper_queue = WhisperQueue::new();

        let mut state = SessionState::Running;
        let mut current_round = 1u32;

        while !state.is_terminal() && current_round <= active_canvas.rounds.max {
            // Round 1 gets no shared_context; subsequent rounds get a graph snapshot
            // rooted at frontier nodes.
            let shared_context = if current_round == 1 {
                None
            } else {
                let frontier_ids: Vec<NodeId> = self.graph.layers.causal.nodes.values()
                    .filter(|n| n.node_type == NodeType::Frontier)
                    .map(|n| n.id.clone())
                    .collect();
                Some(self.graph.retrieve(&frontier_ids, 1))
            };

            let mut runner = RoundRunner {
                round_num: current_round,
                participants: &mut participants,
                context_builder: &mut context_builder,
                whisper_queue: &mut whisper_queue,
                budget: &mut budget,
                transport: &*self.transport,
            };

            let result = runner.run(shared_context).await?;

            // ── Round-boundary graph update ───────────────────────────────────

            // 1. Apply agent proposals (pure conversion, no API call).
            let proposals_delta = proposals_to_delta(result.proposal_ops);
            self.graph.apply_delta(proposals_delta);

            // 2. Extract additional delta from transcript via Haiku (non-fatal).
            if !result.round_transcript.is_empty() {
                let existing_node_ids: Vec<NodeId> = self.graph.layers.causal.nodes.keys()
                    .chain(self.graph.layers.epistemic.nodes.keys())
                    .chain(self.graph.layers.semantic.nodes.keys())
                    .chain(self.graph.layers.economic.nodes.keys())
                    .cloned()
                    .collect();

                match extract_delta(&result.round_transcript, &existing_node_ids, None).await {
                    Ok(extraction_delta) => {
                        self.graph.apply_delta(extraction_delta);
                    }
                    Err(e) => {
                        tracing::warn!("graph extraction failed (non-fatal): {}", e);
                    }
                }
            }

            // ─────────────────────────────────────────────────────────────────

            // Apply canvas hot-switch if requested
            if let Some(new_canvas_id) = result.canvas_switch {
                if let Some(new_canvas) = self.canvas_library.get(&new_canvas_id) {
                    active_canvas = new_canvas.clone();
                    // Re-initialize participants from new canvas
                    participants = active_canvas.initial_participants.iter()
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
                } else {
                    tracing::warn!("SWITCH_CANVAS: canvas '{}' not found in library", new_canvas_id);
                }
            }

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

            if result.should_close || current_round >= active_canvas.rounds.min {
                if result.should_close { break; }
            }

            current_round += 1;
        }

        // Synthesis phase
        state = SessionState::Synthesizing;
        self.transport.broadcast(&SessionEvent::SynthesisStarted).await?;

        let artifacts = synthesize_artifacts(
            &active_canvas.output.sections,
            &context_builder,
            &active_canvas.id,
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
            canvas_id: active_canvas.id.clone(),
            goal: self.goal.clone(),
            artifacts,
            total_tokens: 0, // TODO: sum from budget ledger
        };

        self.transport.emit_output(&output).await?;

        let _ = state; // state variable used for AwaitingApproval tracking
        Ok(output)
    }
}

// ── Graph helpers ─────────────────────────────────────────────────────────────

/// Convert agent-proposed ops into a `GraphDelta` without any API call.
/// Pure conversion: NewNode → new_nodes, NewEdge → new_edges, etc.
fn proposals_to_delta(ops: Vec<ProposalOp>) -> GraphDelta {
    let mut new_nodes = Vec::new();
    let mut new_edges = Vec::new();
    let mut new_vias  = Vec::new();
    let mut updates   = Vec::new();

    for op in ops {
        match op {
            ProposalOp::NewNode { node }                                => new_nodes.push(node),
            ProposalOp::NewEdge { edge }                                => new_edges.push(edge),
            ProposalOp::NewVia  { via }                                 => new_vias.push(via),
            ProposalOp::UpdateNode { id, activation_weight, epistemic_state } => {
                updates.push(NodeUpdate { id, activation_weight, epistemic_state });
            }
        }
    }

    GraphDelta { new_nodes, new_edges, new_vias, updates }
}
