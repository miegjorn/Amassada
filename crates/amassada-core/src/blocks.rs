use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, EdgeType, LayerKind, Node, NodeId, NodeType, Via, ViaType};

// ── ProposalOp ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProposalOp {
    UpdateNode { id: NodeId, activation_weight: Option<f32>, epistemic_state: Option<f32> },
    NewNode    { node: Node },
    NewEdge    { edge: Edge },
    NewVia     { via: Via },
}

// ── AgentBlock ────────────────────────────────────────────────────────────────

/// `Eq` is intentionally absent: `GraphProposal` carries `f32` fields via
/// `ProposalOp` → `Node`/`Edge`/`Via`, which implement only `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentBlock {
    Main { content: String },
    Btw { to: String, content: String },
    Consult { to: String, content: String },
    Leave,
    GraphProposal { ops: Vec<ProposalOp> },
}

// ── ModeratorAction ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeratorAction {
    Invite { agent_id: String },
    Release { agent_id: String },
    ForkConsultation { agent_a: String, agent_b: String, topic: String },
    AdjustBudget { pool: String, delta: i64 },
    RequestApproval { reason: String },
    SetModel { model: String, for_agent: String },
    Close,
    SwitchCanvas { canvas_id: String },
}

// ── ParsedResponse ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParsedResponse {
    pub agent_blocks: Vec<AgentBlock>,
    pub moderator_actions: Vec<ModeratorAction>,
    pub raw: String,
}

// ── parse_blocks ──────────────────────────────────────────────────────────────

pub fn parse_blocks(input: &str, is_moderator: bool) -> ParsedResponse {
    let mut agent_blocks = Vec::new();
    let mut moderator_actions = Vec::new();

    // Split by block header patterns: lines starting with [BLOCK...]
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("[MAIN]") {
            // collect until next block header
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Main { content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[BTW to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Btw { to, content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[CONSULT to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Consult { to, content });
            i += 1;
        } else if line.starts_with("[LEAVE]") {
            agent_blocks.push(AgentBlock::Leave);
            i += 1;
        } else if line.starts_with("[GRAPH_PROPOSAL]") {
            let content = collect_until_next_block(&lines, i + 1);
            let ops = parse_graph_proposal_content(&content);
            agent_blocks.push(AgentBlock::GraphProposal { ops });
            i += 1;
        } else if is_moderator {
            if let Some(id) = extract_param(line, "[INVITE:") {
                moderator_actions.push(ModeratorAction::Invite { agent_id: id });
                i += 1;
            } else if let Some(id) = extract_param(line, "[RELEASE:") {
                moderator_actions.push(ModeratorAction::Release { agent_id: id });
                i += 1;
            } else if line.starts_with("[CLOSE]") {
                moderator_actions.push(ModeratorAction::Close);
                i += 1;
            } else if let Some(reason) = extract_param(line, "[REQUEST_APPROVAL:") {
                moderator_actions.push(ModeratorAction::RequestApproval { reason });
                i += 1;
            } else if line.starts_with("[ADJUST_BUDGET:") {
                // [ADJUST_BUDGET: main_session, -10000]
                let inner = line.trim_start_matches("[ADJUST_BUDGET:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let pool = parts[0].trim().to_string();
                    let delta: i64 = parts[1].trim().parse().unwrap_or(0);
                    moderator_actions.push(ModeratorAction::AdjustBudget { pool, delta });
                }
                i += 1;
            } else if line.starts_with("[MODEL:") {
                // [MODEL: claude-sonnet-4-6 for: builder]
                let inner = line.trim_start_matches("[MODEL:").trim_end_matches(']');
                if let Some((model, for_part)) = inner.split_once(" for:") {
                    moderator_actions.push(ModeratorAction::SetModel {
                        model: model.trim().to_string(),
                        for_agent: for_part.trim().to_string(),
                    });
                }
                i += 1;
            } else if line.starts_with("[FORK_CONSULTATION:") {
                let inner = line.trim_start_matches("[FORK_CONSULTATION:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(3, ',').collect();
                if parts.len() == 3 {
                    moderator_actions.push(ModeratorAction::ForkConsultation {
                        agent_a: parts[0].trim().to_string(),
                        agent_b: parts[1].trim().to_string(),
                        topic: parts[2].trim().to_string(),
                    });
                }
                i += 1;
            } else if line.starts_with("[SWITCH_CANVAS:") {
                let id = line.trim_start_matches("[SWITCH_CANVAS:").trim_end_matches(']').trim().to_string();
                moderator_actions.push(ModeratorAction::SwitchCanvas { canvas_id: id });
                i += 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    ParsedResponse { agent_blocks, moderator_actions, raw: input.to_string() }
}

// ── GRAPH_PROPOSAL content parser ─────────────────────────────────────────────

fn parse_graph_proposal_content(content: &str) -> Vec<ProposalOp> {
    let mut ops = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("UPDATE ") {
            match parse_update_op(rest) {
                Some(op) => ops.push(op),
                None => tracing::warn!("[GRAPH_PROPOSAL] invalid UPDATE line: {}", line),
            }
        } else if let Some(rest) = line.strip_prefix("NEW node: ") {
            match parse_new_node_op(rest) {
                Some(op) => ops.push(op),
                None => tracing::warn!("[GRAPH_PROPOSAL] invalid NEW node line: {}", line),
            }
        } else if let Some(rest) = line.strip_prefix("EDGE ") {
            match parse_edge_op(rest) {
                Some(op) => ops.push(op),
                None => tracing::warn!("[GRAPH_PROPOSAL] invalid EDGE line: {}", line),
            }
        } else if let Some(rest) = line.strip_prefix("VIA ") {
            match parse_via_op(rest) {
                Some(op) => ops.push(op),
                None => tracing::warn!("[GRAPH_PROPOSAL] invalid VIA line: {}", line),
            }
        } else {
            tracing::warn!("[GRAPH_PROPOSAL] unrecognized op line: {}", line);
        }
    }

    ops
}

/// Parse `UPDATE <id>: activation_weight=0.95, epistemic_state=0.80`
/// (the `UPDATE ` prefix has already been stripped by the caller).
fn parse_update_op(s: &str) -> Option<ProposalOp> {
    // s = "N1: activation_weight=0.95, epistemic_state=0.80"
    let (id_str, kv_str) = s.split_once(": ")?;
    let id = NodeId(id_str.trim().to_string());
    let kv = parse_kv_pairs(kv_str);
    let activation_weight = kv.get("activation_weight").and_then(|v| v.parse().ok());
    let epistemic_state   = kv.get("epistemic_state").and_then(|v| v.parse().ok());
    Some(ProposalOp::UpdateNode { id, activation_weight, epistemic_state })
}

/// Parse `id=N3, type=resolved, summary="...", layer=causal, activation_weight=0.8, epistemic_state=0.05`
/// (the `NEW node: ` prefix has already been stripped by the caller).
///
/// `id` is required; returns `None` if absent (triggers caller warning).
/// `layer=semantic` overrides `node_type` to `Supporting` (same routing rule as the extractor).
fn parse_new_node_op(s: &str) -> Option<ProposalOp> {
    let kv = parse_kv_pairs(s);

    let id      = NodeId(kv.get("id")?.trim().to_string());
    let summary = kv.get("summary").cloned().unwrap_or_default();
    let layer   = kv.get("layer").map(|s| s.as_str()).unwrap_or("");
    let raw_type = kv.get("type").map(|s| s.as_str()).unwrap_or("frontier");

    // Honour layer=semantic → Supporting (mirrors extractor routing).
    let node_type = if layer == "semantic" {
        NodeType::Supporting
    } else {
        parse_node_type_str(raw_type)
    };

    let activation_weight = kv.get("activation_weight")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);
    let epistemic_state = kv.get("epistemic_state")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);

    Some(ProposalOp::NewNode {
        node: Node { id, summary, node_type, activation_weight, epistemic_state, farga_ref: None },
    })
}

/// Parse `N1->N2: leads_to weight=1.0`
/// (the `EDGE ` prefix has already been stripped by the caller).
fn parse_edge_op(s: &str) -> Option<ProposalOp> {
    // s = "N1->N2: leads_to weight=1.0"
    let (endpoints, descriptor) = s.split_once(": ")?;
    let (from_str, to_str) = endpoints.split_once("->")?;

    // descriptor = "leads_to weight=1.0"
    let (edge_type_str, rest) = descriptor.split_once(' ')
        .unwrap_or((descriptor, ""));
    let weight = parse_kv_pairs(rest)
        .get("weight")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);

    Some(ProposalOp::NewEdge {
        edge: Edge {
            from:      NodeId(from_str.trim().to_string()),
            to:        NodeId(to_str.trim().to_string()),
            edge_type: parse_edge_type_str(edge_type_str.trim()),
            weight,
        },
    })
}

/// Parse `causal:N1->semantic:S1: analogy_of strength=0.70`
/// (the `VIA ` prefix has already been stripped by the caller).
fn parse_via_op(s: &str) -> Option<ProposalOp> {
    // s = "causal:N1->semantic:S1: analogy_of strength=0.70"
    // Split on "->" to isolate from-endpoint and the rest.
    let (from_part, right) = s.split_once("->")?;

    // right = "semantic:S1: analogy_of strength=0.70"
    // Split on ": " to separate to-endpoint from the via descriptor.
    let (to_endpoint, descriptor) = right.split_once(": ")?;

    let (from_layer_str, from_node_str) = from_part.trim().split_once(':')?;
    let (to_layer_str,   to_node_str)   = to_endpoint.trim().split_once(':')?;

    // descriptor = "analogy_of strength=0.70"
    let (via_type_str, kv_rest) = descriptor.split_once(' ')
        .unwrap_or((descriptor, ""));
    let strength = parse_kv_pairs(kv_rest)
        .get("strength")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);

    Some(ProposalOp::NewVia {
        via: Via {
            from_layer: parse_layer_kind_str(from_layer_str.trim()),
            from_node:  NodeId(from_node_str.trim().to_string()),
            to_layer:   parse_layer_kind_str(to_layer_str.trim()),
            to_node:    NodeId(to_node_str.trim().to_string()),
            via_type:   parse_via_type_str(via_type_str.trim()),
            strength,
        },
    })
}

// ── KV pair parser ────────────────────────────────────────────────────────────

/// Parse a comma-separated `key=value` string.
///
/// Values may be double-quoted (e.g. `summary="some text with spaces"`).
/// Quoted values may contain commas; unquoted values are terminated by the
/// next comma.
fn parse_kv_pairs(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut remaining = s.trim();

    while !remaining.is_empty() {
        // Find '=' to split key from value.
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };

        // Key is the slice before '=', stripped of any leading comma/whitespace
        // left over from the previous iteration.
        let key = remaining[..eq_pos]
            .trim_matches(|c: char| c == ',' || c.is_whitespace())
            .to_string();

        remaining = &remaining[eq_pos + 1..];

        // Parse value.
        if remaining.starts_with('"') {
            // Quoted value — consume up to the closing double-quote.
            remaining = &remaining[1..];
            match remaining.find('"') {
                Some(end) => {
                    if !key.is_empty() {
                        map.insert(key, remaining[..end].to_string());
                    }
                    remaining = remaining[end + 1..]
                        .trim_start_matches(|c: char| c == ',' || c.is_whitespace());
                }
                None => {
                    // Unterminated quote — treat rest as the value and stop.
                    if !key.is_empty() {
                        map.insert(key, remaining.to_string());
                    }
                    break;
                }
            }
        } else {
            // Unquoted value — terminated by the next comma.
            match remaining.find(',') {
                Some(comma_pos) => {
                    let value = remaining[..comma_pos].trim().to_string();
                    if !key.is_empty() {
                        map.insert(key, value);
                    }
                    remaining = remaining[comma_pos + 1..].trim_start();
                }
                None => {
                    let value = remaining.trim().to_string();
                    if !key.is_empty() {
                        map.insert(key, value);
                    }
                    break;
                }
            }
        }
    }

    map
}

// ── String → graph enum helpers ───────────────────────────────────────────────

fn parse_node_type_str(s: &str) -> NodeType {
    match s {
        "axiom"      => NodeType::Axiom,
        "resolved"   => NodeType::Resolved,
        "question"   => NodeType::Question,
        "supporting" => NodeType::Supporting,
        "frontier"   => NodeType::Frontier,
        "dead"       => NodeType::Dead,
        _            => NodeType::Frontier,
    }
}

fn parse_edge_type_str(s: &str) -> EdgeType {
    match s {
        "leads_to"   => EdgeType::LeadsTo,
        "supports"   => EdgeType::Supports,
        "supersedes" => EdgeType::Supersedes,
        "challenges" => EdgeType::Challenges,
        "dead"       => EdgeType::Dead,
        _            => EdgeType::LeadsTo,
    }
}

fn parse_layer_kind_str(s: &str) -> LayerKind {
    match s {
        "causal"    => LayerKind::Causal,
        "epistemic" => LayerKind::Epistemic,
        "semantic"  => LayerKind::Semantic,
        "economic"  => LayerKind::Economic,
        _           => LayerKind::Causal,
    }
}

fn parse_via_type_str(s: &str) -> ViaType {
    match s {
        "analogy_of" => ViaType::AnalogyOf,
        "similar_to" => ViaType::SimilarTo,
        "grounds"    => ViaType::Grounds,
        "challenges" => ViaType::Challenges,
        _            => ViaType::AnalogyOf,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn extract_param(line: &str, prefix: &str) -> Option<String> {
    if line.starts_with(prefix) {
        let inner = line[prefix.len()..].trim_end_matches(']');
        Some(inner.trim().to_string())
    } else {
        None
    }
}

fn collect_until_next_block(lines: &[&str], start: usize) -> String {
    let block_starters = [
        "[MAIN]", "[BTW to:", "[CONSULT to:", "[LEAVE]", "[GRAPH_PROPOSAL]",
        "[INVITE:", "[RELEASE:", "[CLOSE]", "[REQUEST_APPROVAL:", "[ADJUST_BUDGET:",
        "[MODEL:", "[FORK_CONSULTATION:", "[SWITCH_CANVAS:",
    ];
    let mut content_lines = Vec::new();
    for &line in &lines[start..] {
        let trimmed = line.trim();
        if block_starters.iter().any(|s| trimmed.starts_with(s)) {
            break;
        }
        content_lines.push(line);
    }
    content_lines.join("\n")
}
