use serde::Deserialize;

use crate::error::{AmassadaError, Result};
use super::{Edge, EdgeType, LayerKind, Node, NodeId, NodeType, Via, ViaType};

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NodeUpdate {
    pub id:                NodeId,
    pub activation_weight: Option<f32>,
    pub epistemic_state:   Option<f32>,
}

/// Delta returned by one Haiku extraction call; applied to a `SessionGraph`
/// via `SessionGraph::apply_delta`.
#[derive(Debug, Clone)]
pub struct GraphDelta {
    pub new_nodes: Vec<Node>,
    pub new_edges: Vec<Edge>,
    pub new_vias:  Vec<Via>,
    pub updates:   Vec<NodeUpdate>,
}

// ── Raw deserialization structs ───────────────────────────────────────────────

#[derive(Deserialize)]
struct RawNode {
    id:                String,
    node_type:         String,
    summary:           String,
    /// Used for layer routing: "semantic" overrides node_type-based routing.
    #[serde(default)]
    layer:             String,
    activation_weight: f32,
    epistemic_state:   f32,
}

#[derive(Deserialize)]
struct RawEdge {
    from:      String,
    to:        String,
    edge_type: String,
    weight:    f32,
}

#[derive(Deserialize)]
struct RawVia {
    from_layer: String,
    from_node:  String,
    to_layer:   String,
    to_node:    String,
    via_type:   String,
    strength:   f32,
}

#[derive(Deserialize)]
struct RawUpdate {
    id:                String,
    activation_weight: Option<f32>,
    epistemic_state:   Option<f32>,
}

#[derive(Deserialize)]
struct RawDelta {
    #[serde(default)]
    nodes:   Vec<RawNode>,
    #[serde(default)]
    edges:   Vec<RawEdge>,
    #[serde(default)]
    vias:    Vec<RawVia>,
    #[serde(default)]
    updates: Vec<RawUpdate>,
}

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Parse `node_type` string from Haiku.  When the `layer` field is "semantic",
/// the caller overrides to `NodeType::Supporting` so that `apply_delta` can
/// route the node to `layers.semantic` by inspecting `node_type` alone.
fn parse_node_type(s: &str) -> NodeType {
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

fn parse_edge_type(s: &str) -> EdgeType {
    match s {
        "leads_to"   => EdgeType::LeadsTo,
        "supports"   => EdgeType::Supports,
        "supersedes" => EdgeType::Supersedes,
        "challenges" => EdgeType::Challenges,
        "dead"       => EdgeType::Dead,
        _            => EdgeType::LeadsTo,
    }
}

fn parse_layer_kind(s: &str) -> LayerKind {
    match s {
        "causal"    => LayerKind::Causal,
        "epistemic" => LayerKind::Epistemic,
        "semantic"  => LayerKind::Semantic,
        "economic"  => LayerKind::Economic,
        _           => LayerKind::Causal,
    }
}

fn parse_via_type(s: &str) -> ViaType {
    match s {
        "analogy_of" => ViaType::AnalogyOf,
        "similar_to" => ViaType::SimilarTo,
        "grounds"    => ViaType::Grounds,
        "challenges" => ViaType::Challenges,
        _            => ViaType::AnalogyOf,
    }
}

// ── Core parsing function (tested directly; no live API needed) ───────────────

/// Parse raw JSON text from a Haiku extraction response into a `GraphDelta`.
///
/// When `layer == "semantic"`, the node_type is promoted to
/// `NodeType::Supporting` so that `apply_delta` routes the node to
/// `layers.semantic` without needing to carry separate layer metadata.
/// Haiku is instructed to return bare JSON, but routinely wraps it in a
/// ```` ```json ... ``` ```` fence (or bare ``` ```` ```` ````) regardless —
/// a well-documented LLM habit, not something the prompt can reliably stop.
/// `serde_json::from_str` on a fenced response fails as "expected value at
/// line 1 column 1" (the backtick isn't valid JSON), which is exactly the
/// non-fatal warning seen firing on nearly every real turn. Strip a
/// surrounding fence, if present, before parsing.
fn strip_json_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(after_open) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let after_open = after_open
        .strip_prefix("json")
        .unwrap_or(after_open)
        .trim_start_matches(['\n', '\r']);
    after_open.strip_suffix("```").unwrap_or(after_open).trim()
}

fn parse_extraction_response(raw: &str) -> Result<GraphDelta> {
    let raw_delta: RawDelta = serde_json::from_str(strip_json_fence(raw))
        .map_err(|e| AmassadaError::Dispatch(format!("extraction parse error: {}", e)))?;

    let new_nodes = raw_delta
        .nodes
        .into_iter()
        .map(|rn| {
            // Honour the `layer` field for routing: if the model says "semantic",
            // encode that as Supporting so apply_delta sends it to layers.semantic.
            let node_type = if rn.layer == "semantic" {
                NodeType::Supporting
            } else {
                parse_node_type(&rn.node_type)
            };
            Node {
                id:                NodeId(rn.id),
                summary:           rn.summary,
                node_type,
                activation_weight: rn.activation_weight,
                epistemic_state:   rn.epistemic_state,
                farga_ref:         None,
            }
        })
        .collect();

    let new_edges = raw_delta
        .edges
        .into_iter()
        .map(|re| Edge {
            from:      NodeId(re.from),
            to:        NodeId(re.to),
            edge_type: parse_edge_type(&re.edge_type),
            weight:    re.weight,
        })
        .collect();

    let new_vias = raw_delta
        .vias
        .into_iter()
        .map(|rv| Via {
            from_layer: parse_layer_kind(&rv.from_layer),
            from_node:  NodeId(rv.from_node),
            to_layer:   parse_layer_kind(&rv.to_layer),
            to_node:    NodeId(rv.to_node),
            via_type:   parse_via_type(&rv.via_type),
            strength:   rv.strength,
        })
        .collect();

    let updates = raw_delta
        .updates
        .into_iter()
        .map(|ru| NodeUpdate {
            id:                NodeId(ru.id),
            activation_weight: ru.activation_weight,
            epistemic_state:   ru.epistemic_state,
        })
        .collect();

    Ok(GraphDelta { new_nodes, new_edges, new_vias, updates })
}

// ── API call ──────────────────────────────────────────────────────────────────

const EXTRACTION_MODEL: &str = "claude-haiku-4-5-20251001";
// For full intermingling, this could be made per-call or config; currently Claude for graph quality. Grok support via endpoint routing for agent contexts.

/// Call Haiku to extract a `GraphDelta` from a raw transcript segment.
///
/// Returns `Err` on network failure, API error, or JSON parse failure.
/// Errors are non-fatal — callers must handle them (log and continue).
pub async fn extract_delta(
    transcript_segment: &str,
    existing_nodes:     &[NodeId],
    api_key:            Option<String>,
) -> Result<GraphDelta> {
    let api_key = api_key
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| AmassadaError::Dispatch("ANTHROPIC_API_KEY not set".into()))?;

    let existing_list = if existing_nodes.is_empty() {
        "none".to_string()
    } else {
        existing_nodes
            .iter()
            .map(|id| id.0.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let system_prompt = format!(
        r#"You are a knowledge-graph extraction engine. Given a transcript segment, extract a graph delta.

Return ONLY valid JSON — no markdown, no code fences, no explanation. Match this schema exactly:
{{
  "nodes":   [{{"id":"N1","node_type":"resolved","summary":"15 words or fewer","layer":"causal","activation_weight":0.8,"epistemic_state":0.05}}],
  "edges":   [{{"from":"N1","to":"N2","edge_type":"leads_to","weight":1.0}}],
  "vias":    [{{"from_layer":"semantic","from_node":"wdm","to_layer":"causal","to_node":"N1","via_type":"analogy_of","strength":0.85}}],
  "updates": [{{"id":"N3","activation_weight":0.95,"epistemic_state":null}}]
}}

Allowed values:
  node_type: axiom | resolved | question | supporting | frontier | dead
  layer:     causal | epistemic | semantic | economic
  edge_type: leads_to | supports | supersedes | challenges | dead
  via_type:  analogy_of | similar_to | grounds | challenges

Use layer "semantic" and node_type "supporting" for nodes that represent semantic relationships.

Existing node IDs you may reference in edges/updates (do NOT recreate as new nodes): {existing_list}

Rules:
- summaries must be 15 words or fewer
- activation_weight and epistemic_state are floats in [0.0, 1.0]
- use null for update fields you are not changing
- emit empty arrays when there is nothing to add"#
    );

    let user_message = format!(
        "Extract the graph delta from this transcript segment:\n\n{transcript_segment}"
    );

    let body = if EXTRACTION_MODEL.starts_with("grok") || EXTRACTION_MODEL.starts_with("xai") {
        let xai_key = std::env::var("XAI_API_KEY").map_err(|_| AmassadaError::Dispatch("XAI_API_KEY not set".into()))?;
        serde_json::json!({
            "model": EXTRACTION_MODEL,
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": format!("{}\n\n{}", system_prompt, user_message)}]
        });
        // call will be adjusted below
        serde_json::json!({})
    } else {
        serde_json::json!({
            "model": EXTRACTION_MODEL,
            "max_tokens": 1024,
            "system": [{"type": "text", "text": system_prompt, "cache_control": {"type": "ephemeral"}}],
            "messages": [{"role": "user", "content": user_message}]
        })
    };

    let client = reqwest::Client::new();
    let resp = if EXTRACTION_MODEL.starts_with("grok") || EXTRACTION_MODEL.starts_with("xai") {
        let xai_key = std::env::var("XAI_API_KEY").map_err(|_| AmassadaError::Dispatch("XAI_API_KEY not set".into()))?;
        client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", xai_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": EXTRACTION_MODEL,
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": format!("{}\n\n{}", system_prompt, user_message)}]
            }))
            .send()
            .await
            .map_err(|e| AmassadaError::Dispatch(format!("extraction request failed: {}", e)))?
    } else {
        client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AmassadaError::Dispatch(format!("extraction request failed: {}", e)))?
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AmassadaError::Dispatch(
            format!("extraction API error {}: {}", status, text),
        ));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AmassadaError::Dispatch(format!("extraction response parse: {}", e)))?;

    let raw_content = json["content"]
        .as_array()
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|b| b["type"].as_str() == Some("text"))
        })
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| {
            AmassadaError::Dispatch(
                "extraction: no text content block in API response".into(),
            )
        })?;

    parse_extraction_response(raw_content)
}

// ── Trajectory-conditioned scope selection ─────────────────────────────────────
//
// Implements the piece named as "not built" in
// `docs/superpowers/specs/2026-07-01-trajectory-conditioned-collapse.md`:
// `f(response) -> next_scope`, replacing the static Frontier-flag scope
// selection in `session.rs` with a selection conditioned on the response just
// produced. The spec's own text defers the trajectory vector's concrete
// representation ("embedding model, decay/recency weighting... deferred; not
// required for Epic 1's single-sensor scope") and ADR-N-002 (the vector
// mechanism the spec proposed reusing) has no matching code anywhere in Nervi
// as of this writing — there is no embedding infrastructure in this stack to
// reuse. This implements the same call-by-call judgment approach `extract_delta`
// above already uses (a cheap Haiku call) rather than inventing vector math
// from scratch: given the response just produced and the graph's current
// nodes, ask which nodes are relevant scope for the next round.

/// Ask Haiku which of `nodes` are relevant scope for the round following
/// `response_text`. Returns `Ok(vec![])` (not an error) when the model finds
/// no relevant nodes — a real "nothing carries forward" judgment, distinct
/// from a call failure. Errors are non-fatal by this crate's convention —
/// callers should fall back to the static Frontier-flag selection.
pub async fn select_scope_from_response(
    response_text: &str,
    nodes: &[Node],
    api_key: Option<String>,
) -> Result<Vec<NodeId>> {
    let api_key = api_key
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| AmassadaError::Dispatch("ANTHROPIC_API_KEY not set".into()))?;

    if nodes.is_empty() {
        return Ok(vec![]);
    }

    let node_list = nodes
        .iter()
        .map(|n| format!("{}: {}", n.id.0, n.summary))
        .collect::<Vec<_>>()
        .join("\n");

    let system_prompt = "You are a context-relevance judge for a multi-round reasoning session. \
Given the response just produced in this round and a list of existing context \
nodes (id: summary), select which node IDs are relevant scope for continuing \
this line of reasoning in the NEXT round — a real trajectory judgment, not a \
static flag. Return ONLY valid JSON, no markdown, no code fences, matching \
this schema exactly: {\"node_ids\": [\"N1\", \"N3\"]}. Return an empty array \
when nothing in the list is relevant to where the response just took the \
conversation.";

    let user_message = format!(
        "Response just produced this round:\n{response_text}\n\nExisting nodes:\n{node_list}\n\nWhich node IDs are relevant scope for the next round?"
    );

    let body = serde_json::json!({
        "model": EXTRACTION_MODEL,
        "max_tokens": 512,
        "system": [{"type": "text", "text": system_prompt, "cache_control": {"type": "ephemeral"}}],
        "messages": [{"role": "user", "content": user_message}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "prompt-caching-2024-07-31")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AmassadaError::Dispatch(format!("scope selection request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AmassadaError::Dispatch(
            format!("scope selection API error {}: {}", status, text),
        ));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AmassadaError::Dispatch(format!("scope selection response parse: {}", e)))?;

    let raw_content = json["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"].as_str() == Some("text")))
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| {
            AmassadaError::Dispatch("scope selection: no text content block in API response".into())
        })?;

    parse_scope_selection_response(raw_content)
}

fn parse_scope_selection_response(raw: &str) -> Result<Vec<NodeId>> {
    #[derive(Deserialize)]
    struct RawScope {
        node_ids: Vec<String>,
    }
    let parsed: RawScope = serde_json::from_str(strip_json_fence(raw))
        .map_err(|e| AmassadaError::Dispatch(format!("scope selection parse error: {}", e)))?;
    Ok(parsed.node_ids.into_iter().map(NodeId).collect())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Full fixture response matching the schema described in the extraction prompt.
    const FIXTURE_JSON: &str = r#"{
        "nodes": [
            {
                "id": "N1",
                "node_type": "resolved",
                "summary": "multi-layer graph enables rich context compression",
                "layer": "causal",
                "activation_weight": 0.8,
                "epistemic_state": 0.05
            }
        ],
        "edges": [
            {"from": "N1", "to": "N2", "edge_type": "leads_to", "weight": 1.0}
        ],
        "vias": [
            {
                "from_layer": "semantic",
                "from_node": "wdm",
                "to_layer": "causal",
                "to_node": "N1",
                "via_type": "analogy_of",
                "strength": 0.85
            }
        ],
        "updates": [
            {"id": "N3", "activation_weight": 0.95, "epistemic_state": null}
        ]
    }"#;

    #[test]
    fn extract_delta_parses_valid_json() {
        let delta = parse_extraction_response(FIXTURE_JSON)
            .expect("fixture JSON must parse without error");

        // nodes
        assert_eq!(delta.new_nodes.len(), 1);
        assert_eq!(delta.new_nodes[0].id, NodeId("N1".to_string()));
        assert_eq!(delta.new_nodes[0].node_type, NodeType::Resolved);
        assert!(
            (delta.new_nodes[0].activation_weight - 0.8).abs() < 1e-6,
            "activation_weight must be 0.8"
        );
        assert!(
            (delta.new_nodes[0].epistemic_state - 0.05).abs() < 1e-6,
            "epistemic_state must be 0.05"
        );

        // edges
        assert_eq!(delta.new_edges.len(), 1);
        assert_eq!(delta.new_edges[0].from, NodeId("N1".to_string()));
        assert_eq!(delta.new_edges[0].to, NodeId("N2".to_string()));
        assert_eq!(delta.new_edges[0].edge_type, EdgeType::LeadsTo);
        assert!((delta.new_edges[0].weight - 1.0).abs() < 1e-6);

        // vias
        assert_eq!(delta.new_vias.len(), 1);
        assert_eq!(delta.new_vias[0].from_layer, LayerKind::Semantic);
        assert_eq!(delta.new_vias[0].from_node, NodeId("wdm".to_string()));
        assert_eq!(delta.new_vias[0].to_layer, LayerKind::Causal);
        assert_eq!(delta.new_vias[0].to_node, NodeId("N1".to_string()));
        assert_eq!(delta.new_vias[0].via_type, ViaType::AnalogyOf);
        assert!(
            (delta.new_vias[0].strength - 0.85).abs() < 1e-6,
            "via strength must be 0.85"
        );

        // updates
        assert_eq!(delta.updates.len(), 1);
        assert_eq!(delta.updates[0].id, NodeId("N3".to_string()));
        assert!(
            delta.updates[0]
                .activation_weight
                .is_some_and(|w| (w - 0.95).abs() < 1e-6),
            "update activation_weight must be 0.95"
        );
        assert!(
            delta.updates[0].epistemic_state.is_none(),
            "null epistemic_state must deserialize as None"
        );
    }

    #[test]
    fn parse_extraction_response_empty_arrays() {
        let json = r#"{"nodes":[],"edges":[],"vias":[],"updates":[]}"#;
        let delta = parse_extraction_response(json).expect("empty arrays must parse");
        assert!(delta.new_nodes.is_empty());
        assert!(delta.new_edges.is_empty());
        assert!(delta.new_vias.is_empty());
        assert!(delta.updates.is_empty());
    }

    #[test]
    fn parse_extraction_response_invalid_json_returns_err() {
        let result = parse_extraction_response("not json at all");
        assert!(result.is_err(), "invalid JSON must return Err");
    }

    #[test]
    fn parse_extraction_response_strips_json_fence() {
        let fenced = "```json\n{\"nodes\":[],\"edges\":[],\"vias\":[],\"updates\":[]}\n```";
        let delta = parse_extraction_response(fenced).expect("fenced JSON must parse");
        assert!(delta.new_nodes.is_empty());
    }

    #[test]
    fn parse_extraction_response_strips_bare_fence() {
        let fenced = "```\n{\"nodes\":[],\"edges\":[],\"vias\":[],\"updates\":[]}\n```";
        let delta = parse_extraction_response(fenced).expect("bare-fenced JSON must parse");
        assert!(delta.new_nodes.is_empty());
    }

    #[test]
    fn parse_extraction_response_semantic_layer_sets_supporting() {
        let json = r#"{
            "nodes": [{"id":"S1","node_type":"axiom","summary":"semantic node forced to supporting","layer":"semantic","activation_weight":0.5,"epistemic_state":0.3}],
            "edges":[],"vias":[],"updates":[]
        }"#;
        let delta = parse_extraction_response(json).expect("must parse");
        assert_eq!(
            delta.new_nodes[0].node_type,
            NodeType::Supporting,
            "layer=semantic must override node_type to Supporting for routing"
        );
    }

    #[test]
    fn parse_scope_selection_response_returns_node_ids() {
        let json = r#"{"node_ids": ["N1", "N3"]}"#;
        let ids = parse_scope_selection_response(json).expect("must parse");
        assert_eq!(ids, vec![NodeId("N1".to_string()), NodeId("N3".to_string())]);
    }

    #[test]
    fn parse_scope_selection_response_empty_array_is_ok_not_err() {
        // A real "nothing carries forward" judgment must parse as Ok(vec![]),
        // distinct from a call/parse failure.
        let json = r#"{"node_ids": []}"#;
        let ids = parse_scope_selection_response(json).expect("empty selection must still parse");
        assert!(ids.is_empty());
    }

    #[test]
    fn parse_scope_selection_response_invalid_json_returns_err() {
        let result = parse_scope_selection_response("not json at all");
        assert!(result.is_err(), "invalid JSON must return Err");
    }

    #[test]
    fn parse_scope_selection_response_strips_json_fence() {
        let fenced = "```json\n{\"node_ids\": [\"N1\"]}\n```";
        let ids = parse_scope_selection_response(fenced).expect("fenced JSON must parse");
        assert_eq!(ids, vec![NodeId("N1".to_string())]);
    }
}
