use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

// ── NodeId ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Enums ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Axiom,
    Resolved,
    Question,
    Supporting,
    Frontier,
    Dead,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NodeType::Axiom     => "axiom",
            NodeType::Resolved  => "resolved",
            NodeType::Question  => "question",
            NodeType::Supporting => "supporting",
            NodeType::Frontier  => "frontier",
            NodeType::Dead      => "dead",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerKind {
    Causal,
    Epistemic,
    Semantic,
    Economic,
}

impl fmt::Display for LayerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            LayerKind::Causal    => "causal",
            LayerKind::Epistemic => "epistemic",
            LayerKind::Semantic  => "semantic",
            LayerKind::Economic  => "economic",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViaType {
    AnalogyOf,
    SimilarTo,
    Grounds,
    Challenges,
}

impl fmt::Display for ViaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ViaType::AnalogyOf  => "analogy_of",
            ViaType::SimilarTo  => "similar_to",
            ViaType::Grounds    => "grounds",
            ViaType::Challenges => "challenges",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    LeadsTo,
    Supports,
    Supersedes,
    Challenges,
    Dead,
}

// ── Structs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id:                NodeId,
    pub summary:           String,    // ≤20 tokens — the pointer text
    pub node_type:         NodeType,
    pub activation_weight: f32,
    pub epistemic_state:   f32,
    pub farga_ref:         Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from:      NodeId,
    pub to:        NodeId,
    pub edge_type: EdgeType,
    pub weight:    f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Via {
    pub from_layer: LayerKind,
    pub from_node:  NodeId,
    pub to_layer:   LayerKind,
    pub to_node:    NodeId,
    pub via_type:   ViaType,
    pub strength:   f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub kind:  LayerKind,
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<Edge>,
}

impl Layer {
    fn new(kind: LayerKind) -> Self {
        Self {
            kind,
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerSet {
    pub causal:    Layer,
    pub epistemic: Layer,
    pub semantic:  Layer,
    pub economic:  Layer,
}

impl LayerSet {
    fn new() -> Self {
        Self {
            causal:    Layer::new(LayerKind::Causal),
            epistemic: Layer::new(LayerKind::Epistemic),
            semantic:  Layer::new(LayerKind::Semantic),
            economic:  Layer::new(LayerKind::Economic),
        }
    }

    fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.causal.nodes.values()
            .chain(self.epistemic.nodes.values())
            .chain(self.semantic.nodes.values())
            .chain(self.economic.nodes.values())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGraph {
    pub version:    u32,
    pub session_id: String,
    pub layers:     LayerSet,
    pub vias:       Vec<Via>,
}

impl SessionGraph {
    pub fn new(session_id: &str) -> Self {
        Self {
            version:    0,
            session_id: session_id.to_string(),
            layers:     LayerSet::new(),
            vias:       Vec::new(),
        }
    }

    /// Serialize the graph to a `SESSION_CONTEXT v0.1` text block.
    ///
    /// Deterministic: nodes are sorted lexicographically by id within each
    /// layer section; vias are sorted by from_node id. Same graph state
    /// always produces identical bytes.
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        // ── header ──────────────────────────────────────────────────────────
        let total_nodes: usize = self.layers.all_nodes().count();

        // frontier = Frontier-typed nodes across all layers, sorted by id
        let mut frontier_ids: Vec<&NodeId> = self
            .layers
            .all_nodes()
            .filter(|n| n.node_type == NodeType::Frontier)
            .map(|n| &n.id)
            .collect();
        frontier_ids.sort_by(|a, b| a.0.cmp(&b.0));

        // unresolved = Question-typed nodes across all layers, sorted by id
        let mut unresolved_ids: Vec<&NodeId> = self
            .layers
            .all_nodes()
            .filter(|n| n.node_type == NodeType::Question)
            .map(|n| &n.id)
            .collect();
        unresolved_ids.sort_by(|a, b| a.0.cmp(&b.0));

        let frontier_str = if frontier_ids.is_empty() {
            "[]".to_string()
        } else {
            format!(
                "[{}]",
                frontier_ids
                    .iter()
                    .map(|id| id.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let unresolved_str = if unresolved_ids.is_empty() {
            "[]".to_string()
        } else {
            format!(
                "[{}]",
                unresolved_ids
                    .iter()
                    .map(|id| id.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        out.push_str("SESSION_CONTEXT v0.1\n");
        out.push_str(&format!(
            "session: {}  round: {}  nodes: {}\n",
            self.session_id, self.version, total_nodes
        ));
        out.push_str(&format!(
            "frontier: {}  unresolved: {}\n",
            frontier_str, unresolved_str
        ));

        // ── CAUSAL section ───────────────────────────────────────────────────
        if !self.layers.causal.nodes.is_empty() {
            out.push('\n');
            out.push_str("CAUSAL\n");
            out.push_str("id        type          aw    ep    summary\n");

            let mut causal_nodes: Vec<&Node> =
                self.layers.causal.nodes.values().collect();
            causal_nodes.sort_by(|a, b| a.id.0.cmp(&b.id.0));

            for node in causal_nodes {
                out.push_str(&format!(
                    "{:<10}  {:<12}  {:.2}  {:.2}  \"{}\"\n",
                    node.id,
                    node.node_type,
                    node.activation_weight,
                    node.epistemic_state,
                    node.summary
                ));
            }
        }

        // ── EPISTEMIC section (open nodes only, epistemic_state < 0.9) ───────
        let mut open_nodes: Vec<&Node> = self
            .layers
            .all_nodes()
            .filter(|n| n.epistemic_state < 0.9)
            .collect();
        open_nodes.sort_by(|a, b| a.id.0.cmp(&b.id.0));

        if !open_nodes.is_empty() {
            out.push('\n');
            out.push_str("EPISTEMIC (open nodes only)\n");
            for node in open_nodes {
                out.push_str(&format!(
                    "{:<10}  {:.2}  \"{}\"\n",
                    node.id, node.epistemic_state, node.summary
                ));
            }
        }

        // ── VIAS section (active: strength >= 0.3) ───────────────────────────
        let mut active_vias: Vec<&Via> =
            self.vias.iter().filter(|v| v.strength >= 0.3).collect();
        active_vias.sort_by(|a, b| a.from_node.0.cmp(&b.from_node.0));

        if !active_vias.is_empty() {
            out.push('\n');
            out.push_str("VIAS (active)\n");
            for via in active_vias {
                out.push_str(&format!(
                    "{} → {}   {}  {:.2}\n",
                    via.from_node, via.to_node, via.via_type, via.strength
                ));
            }
        }

        out
    }
}
