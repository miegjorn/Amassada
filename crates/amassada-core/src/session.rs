use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use crate::blocks::ProposalOp;
use crate::budget::BudgetLedger;
use crate::canvas::Canvas;
use crate::channels::whisper::WhisperQueue;
use crate::context::ContextBuilder;
use crate::error::Result;
use crate::farga;
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
    farga_base_url: Option<String>,
    /// Path to the Fondament repo root. When set, participant domain strings are
    /// resolved via fondament::resolve_persona before building system prompts.
    fondament_path: Option<String>,
}

impl SessionEngine {
    /// Create a new session with a fresh `SessionGraph`.
    /// If `farga_base_url` is `Some`, the graph is persisted to Farga at
    /// session end.
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
            farga_base_url: None,
            fondament_path: None,
        }
    }

    pub fn with_fondament(mut self, path: impl Into<String>) -> Self {
        self.fondament_path = Some(path.into());
        self
    }

    /// Attach a Farga base URL.  When set, the graph is loaded at session
    /// start (if a prior run exists) and saved at session end.  Call before
    /// `run()`.
    pub fn with_farga(mut self, base_url: impl Into<String>) -> Self {
        self.farga_base_url = Some(base_url.into());
        self
    }

    /// Load an existing session from Farga (for continuing sessions).
    ///
    /// If Farga is unreachable or the session has no prior graph, falls back
    /// to a fresh `SessionGraph`.  Always non-fatal.
    pub async fn load(
        session_id: impl Into<String>,
        canvas: Canvas,
        goal: String,
        transport: Arc<dyn Transport>,
        farga_base_url: impl Into<String>,
    ) -> Self {
        let session_id = session_id.into();
        let farga_base_url = farga_base_url.into();
        let graph = farga::load_graph(&farga_base_url, &session_id)
            .await
            .unwrap_or_else(|| {
                tracing::warn!(
                    "farga: no graph found for session {}, starting fresh",
                    session_id
                );
                SessionGraph::new(&session_id)
            });

        Self {
            session_id,
            canvas,
            goal,
            graph,
            transport,
            canvas_library: HashMap::new(),
            farga_base_url: Some(farga_base_url),
            fondament_path: None,
        }
    }

    pub fn with_canvas_library(mut self, library: HashMap<String, Canvas>) -> Self {
        self.canvas_library = library;
        self
    }

    /// Resolve a participant's domain string to actual Fondament context + composition parts.
    /// Falls back to the raw domain string when fondament_path is unset or resolution fails.
    fn resolve_participant_domain(&self, domain: &str)
        -> (String, Vec<fondament_core::types::ComposedPart>)
    {
        if let Some(ref fp) = self.fondament_path {
            match crate::fondament::resolve_persona(fp, domain) {
                Ok(resolved) => return (resolved.context, resolved.collected_parts),
                Err(e) => tracing::debug!("fondament domain '{}' not resolved ({}), using raw string", domain, e),
            }
        }
        (domain.to_string(), vec![])
    }

    pub async fn run(&mut self) -> Result<SessionOutput> {
        // Use a local mutable canvas so hot-switch mid-session is possible
        let mut active_canvas = self.canvas.clone();

        self.transport.broadcast(&SessionEvent::SessionStarted {
            canvas_id: active_canvas.id.clone(),
            goal: self.goal.clone(),
        }).await?;

        // Assemble participants from canvas definitions, resolving Fondament domain context
        // when fondament_path is set so inline (non-endpoint) participants get real personas.
        let mut participants: Vec<ActiveParticipant> = active_canvas.initial_participants.iter()
            .enumerate()
            .map(|(i, def)| {
                let (domain, collected_parts) = self.resolve_participant_domain(&def.domain);
                ActiveParticipant {
                    agent_id: AgentId::new(&format!("{}-{}", def.persona, i)),
                    persona: def.persona.clone(),
                    domain,
                    turns_taken: 0,
                    is_moderator: def.is_moderator(),
                    model: def.model.clone(),
                    thinking_budget: def.thinking_budget,
                    is_deconstructive: def.is_deconstructive(),
                    endpoint: def.endpoint.clone(),
                    context_seal: def.is_sealed(),
                    collected_parts,
                }
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
            // Skip shared_context only when the graph is genuinely empty (new
            // session, graph.version == 0).  For resumed sessions graph.version > 0
            // on entry, so round 1 of a resumed session must still receive the
            // existing graph content.  Also guard against an empty frontier set —
            // retrieve(&[], 1) returns a sparse header-only blob that wastes a
            // cache slot without providing useful context.
            let shared_context = if current_round == 1 && self.graph.version == 0 {
                None
            } else {
                let frontier_ids: Vec<NodeId> = self.graph.layers.causal.nodes.values()
                    .filter(|n| n.node_type == NodeType::Frontier)
                    .map(|n| n.id.clone())
                    .collect();
                if frontier_ids.is_empty() {
                    None
                } else {
                    Some(self.graph.retrieve(&frontier_ids, 1))
                }
            };

            let mut runner = RoundRunner {
                round_num: current_round,
                participants: &mut participants,
                context_builder: &mut context_builder,
                whisper_queue: &mut whisper_queue,
                budget: &mut budget,
                transport: &*self.transport,
                graph: &self.graph,
            };

            let result = runner.run(shared_context).await?;

            // ── Round-boundary graph update ───────────────────────────────────

            // 1. Apply proposals: agent proposals first, then moderator proposals.
            //    Because apply_delta is last-write-wins, applying moderator last
            //    gives moderator precedence on any conflicting node/edge update.
            self.graph.apply_delta(proposals_to_delta(result.agent_proposal_ops));
            self.graph.apply_delta(proposals_to_delta(result.moderator_proposal_ops));

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
                    // Re-initialize participants from new canvas with domain resolution
                    participants = active_canvas.initial_participants.iter()
                        .enumerate()
                        .map(|(i, def)| {
                            let (domain, collected_parts) = self.resolve_participant_domain(&def.domain);
                            ActiveParticipant {
                                agent_id: AgentId::new(&format!("{}-{}", def.persona, i)),
                                persona: def.persona.clone(),
                                domain,
                                turns_taken: 0,
                                is_moderator: def.is_moderator(),
                                model: def.model.clone(),
                                thinking_budget: def.thinking_budget,
                                is_deconstructive: def.is_deconstructive(),
                                endpoint: def.endpoint.clone(),
                                context_seal: def.is_sealed(),
                                collected_parts,
                            }
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
            total_tokens: 0, // Known gap: token aggregation from the budget ledger is not yet
                             // implemented. BudgetLedger tracks per-pool consumption internally
                             // but SessionOutput has no path to read it out. This field is
                             // always 0 in production until that plumbing is added.
        };

        self.transport.emit_output(&output).await?;

        // ── Farga persistence ─────────────────────────────────────────────────
        // Non-fatal: errors are logged inside save_graph; session output is
        // returned regardless.
        if let Some(ref base_url) = self.farga_base_url {
            farga::save_graph(base_url, &self.session_id, &self.graph).await;
        }

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
