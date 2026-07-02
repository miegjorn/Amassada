pub mod extractor;
pub use extractor::{extract_delta, select_scope_from_response, GraphDelta, NodeUpdate};

use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id:                NodeId,
    pub summary:           String,    // ≤20 tokens — the pointer text
    pub node_type:         NodeType,
    pub activation_weight: f32,
    pub epistemic_state:   f32,
    pub farga_ref:         Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub from:      NodeId,
    pub to:        NodeId,
    pub edge_type: EdgeType,
    pub weight:    f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// Return a serialized subset of the graph rooted at `scope`.
    ///
    /// Traversal rules:
    /// - Scope nodes always included.
    /// - Causal-layer `LeadsTo` / `Supports` edges from scope nodes: include
    ///   the `to` node (1 hop).
    /// - Vias with `strength > 0.5` from scope nodes: include via-target node.
    /// - If `via_hops == 2`, follow one additional hop of vias (strength > 0.5)
    ///   from the first-hop via targets.
    ///
    /// Output format is identical to `serialize()` except:
    /// - Only retrieved nodes appear in CAUSAL / EPISTEMIC sections.
    /// - EPISTEMIC threshold is `epistemic_state < 0.8` (stricter than full).
    /// - VIAS includes only vias where at least one endpoint is retrieved.
    pub fn retrieve(&self, scope: &[NodeId], via_hops: u8) -> String {
        let mut retrieved: HashSet<NodeId> = HashSet::new();

        // ── 1. Seed with scope nodes ─────────────────────────────────────────
        for id in scope {
            retrieved.insert(id.clone());
        }

        // ── 2. Causal-layer 1-hop edges from scope nodes ─────────────────────
        let scope_set: HashSet<NodeId> = scope.iter().cloned().collect();
        for edge in &self.layers.causal.edges {
            if scope_set.contains(&edge.from)
                && matches!(edge.edge_type, EdgeType::LeadsTo | EdgeType::Supports)
            {
                retrieved.insert(edge.to.clone());
            }
        }

        // ── 3. Vias with strength > 0.5 from scope nodes ─────────────────────
        let mut via_targets: HashSet<NodeId> = HashSet::new();
        for via in &self.vias {
            if via.strength > 0.5 && scope_set.contains(&via.from_node) {
                via_targets.insert(via.to_node.clone());
                retrieved.insert(via.to_node.clone());
            }
        }

        // ── 4. Optional second hop from via targets ───────────────────────────
        if via_hops == 2 {
            for via in &self.vias {
                if via.strength > 0.5 && via_targets.contains(&via.from_node) {
                    retrieved.insert(via.to_node.clone());
                }
            }
        }

        self.serialize_subset(&retrieved)
    }

    /// Apply a `GraphDelta` returned by the extractor to this graph.
    ///
    /// Routing rule (derives from `node_type` so no separate layer metadata needed):
    /// - `NodeType::Supporting` → `layers.semantic`
    /// - All other node types   → `layers.causal`
    ///
    /// New edges go to `layers.causal.edges`. New vias go to `self.vias`.
    /// Updates are applied to whichever layer contains the target node.
    /// Bumps `self.version` by 1 regardless of delta content.
    pub fn apply_delta(&mut self, delta: extractor::GraphDelta) {
        // ── insert nodes ─────────────────────────────────────────────────────
        for node in delta.new_nodes {
            match node.node_type {
                NodeType::Supporting => {
                    self.layers.semantic.nodes.insert(node.id.clone(), node);
                }
                _ => {
                    self.layers.causal.nodes.insert(node.id.clone(), node);
                }
            }
        }

        // ── insert edges (causal layer) ──────────────────────────────────────
        for edge in delta.new_edges {
            self.layers.causal.edges.push(edge);
        }

        // ── insert vias ──────────────────────────────────────────────────────
        for via in delta.new_vias {
            self.vias.push(via);
        }

        // ── apply updates (search all layers) ────────────────────────────────
        for update in delta.updates {
            let node = self
                .layers
                .causal
                .nodes
                .get_mut(&update.id)
                .or_else(|| self.layers.semantic.nodes.get_mut(&update.id))
                .or_else(|| self.layers.epistemic.nodes.get_mut(&update.id))
                .or_else(|| self.layers.economic.nodes.get_mut(&update.id));

            if let Some(node) = node {
                if let Some(aw) = update.activation_weight {
                    node.activation_weight = aw;
                }
                if let Some(es) = update.epistemic_state {
                    node.epistemic_state = es;
                }
            }
        }

        self.version += 1;
    }

    /// Serialize only the nodes present in `ids`.
    ///
    /// Same `SESSION_CONTEXT v0.1` format as `serialize()`, but:
    /// - CAUSAL section: only retrieved causal-layer nodes.
    /// - EPISTEMIC section: only retrieved nodes with `epistemic_state < 0.8`.
    /// - VIAS section: only active vias (`strength >= 0.3`) where at least one
    ///   endpoint is in `ids`.
    fn serialize_subset(&self, ids: &HashSet<NodeId>) -> String {
        let mut out = String::new();

        // ── header ───────────────────────────────────────────────────────────
        let total_nodes = self.layers.all_nodes().filter(|n| ids.contains(&n.id)).count();

        let mut frontier_ids: Vec<&NodeId> = self
            .layers
            .all_nodes()
            .filter(|n| ids.contains(&n.id) && n.node_type == NodeType::Frontier)
            .map(|n| &n.id)
            .collect();
        frontier_ids.sort_by(|a, b| a.0.cmp(&b.0));

        let mut unresolved_ids: Vec<&NodeId> = self
            .layers
            .all_nodes()
            .filter(|n| ids.contains(&n.id) && n.node_type == NodeType::Question)
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

        // ── CAUSAL section (retrieved causal nodes only) ──────────────────────
        let mut causal_nodes: Vec<&Node> = self
            .layers
            .causal
            .nodes
            .values()
            .filter(|n| ids.contains(&n.id))
            .collect();
        causal_nodes.sort_by(|a, b| a.id.0.cmp(&b.id.0));

        if !causal_nodes.is_empty() {
            out.push('\n');
            out.push_str("CAUSAL\n");
            out.push_str("id        type          aw    ep    summary\n");
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

        // ── EPISTEMIC section (retrieved, epistemic_state < 0.8) ─────────────
        let mut open_nodes: Vec<&Node> = self
            .layers
            .all_nodes()
            .filter(|n| ids.contains(&n.id) && n.epistemic_state < 0.8)
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

        // ── VIAS section (active, at least one endpoint retrieved) ───────────
        // Output threshold (0.3) is intentionally lower than the traversal threshold (0.5):
        // weak vias are visible here for human review even though they are not followed
        // during graph traversal in `retrieve`.  A via line may therefore name a node
        // that was not pulled into the retrieved set.
        let mut active_vias: Vec<&Via> = self
            .vias
            .iter()
            .filter(|v| {
                v.strength >= 0.3
                    && (ids.contains(&v.from_node) || ids.contains(&v.to_node))
            })
            .collect();
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::extractor::GraphDelta;

    /// Serialize a populated `SessionGraph` to JSON, deserialize it back, and
    /// verify structural equality.  No live HTTP calls — pure serde roundtrip.
    #[test]
    fn session_graph_roundtrips_serde() {
        let mut graph = SessionGraph::new("test-session-42");

        // Add a couple of nodes via a delta so we exercise the full path.
        let delta = GraphDelta {
            new_nodes: vec![
                Node {
                    id: NodeId("n1".into()),
                    summary: "first node".into(),
                    node_type: NodeType::Axiom,
                    activation_weight: 0.8,
                    epistemic_state: 0.9,
                    farga_ref: None,
                },
                Node {
                    id: NodeId("n2".into()),
                    summary: "frontier node".into(),
                    node_type: NodeType::Frontier,
                    activation_weight: 0.5,
                    epistemic_state: 0.4,
                    farga_ref: Some("farga://nodes/n2".into()),
                },
            ],
            new_edges: vec![Edge {
                from: NodeId("n1".into()),
                to: NodeId("n2".into()),
                edge_type: EdgeType::LeadsTo,
                weight: 1.0,
            }],
            new_vias: vec![],
            updates: vec![],
        };
        graph.apply_delta(delta);

        // Serialize to JSON
        let json = serde_json::to_string(&graph).expect("serialize should succeed");

        // Deserialize back
        let restored: SessionGraph =
            serde_json::from_str(&json).expect("deserialize should succeed");

        // Structural checks
        assert_eq!(restored.session_id, "test-session-42");
        assert_eq!(restored.version, graph.version);
        assert_eq!(
            restored.layers.causal.nodes.len(),
            graph.layers.causal.nodes.len()
        );
        assert_eq!(restored.layers.causal.edges.len(), 1);

        // Spot-check a node
        let n2 = restored
            .layers
            .causal
            .nodes
            .get(&NodeId("n2".into()))
            .expect("n2 should survive roundtrip");
        assert_eq!(n2.node_type, NodeType::Frontier);
        assert_eq!(n2.farga_ref.as_deref(), Some("farga://nodes/n2"));
    }
}
