use amassada_core::graph::*;

// ── retrieve helpers ──────────────────────────────────────────────────────────

fn make_node(id: &str, summary: &str, node_type: NodeType, ep: f32) -> (NodeId, Node) {
    let nid = NodeId(id.to_string());
    let node = Node {
        id: nid.clone(),
        summary: summary.to_string(),
        node_type,
        activation_weight: 0.8,
        epistemic_state: ep,
        farga_ref: None,
    };
    (nid, node)
}

fn make_edge(from: &str, to: &str, edge_type: EdgeType) -> Edge {
    Edge {
        from: NodeId(from.to_string()),
        to: NodeId(to.to_string()),
        edge_type,
        weight: 0.8,
    }
}

fn make_via(from_node: &str, to_node: &str, strength: f32) -> Via {
    Via {
        from_layer: LayerKind::Causal,
        from_node: NodeId(from_node.to_string()),
        to_layer: LayerKind::Semantic,
        to_node: NodeId(to_node.to_string()),
        via_type: ViaType::AnalogyOf,
        strength,
    }
}

// ── core types ──────────────────────────────────────────────────────────────

#[test]
fn session_graph_initializes_empty() {
    let graph = SessionGraph::new("test-session");
    assert_eq!(graph.version, 0);
    assert_eq!(graph.session_id, "test-session");
    assert!(graph.layers.causal.nodes.is_empty());
    assert!(graph.layers.epistemic.nodes.is_empty());
    assert!(graph.layers.semantic.nodes.is_empty());
    assert!(graph.layers.economic.nodes.is_empty());
    assert!(graph.vias.is_empty());
}

#[test]
fn node_id_equality() {
    let a = NodeId("same".to_string());
    let b = NodeId("same".to_string());
    let c = NodeId("different".to_string());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn layer_insert_and_retrieve() {
    let mut graph = SessionGraph::new("test-session");
    let id = NodeId("N1".to_string());
    let node = Node {
        id: id.clone(),
        summary: "test node summary".to_string(),
        node_type: NodeType::Axiom,
        activation_weight: 0.8,
        epistemic_state: 0.5,
        farga_ref: None,
    };
    graph.layers.causal.nodes.insert(id.clone(), node);
    let retrieved = graph.layers.causal.nodes.get(&id).unwrap();
    assert_eq!(retrieved.summary, "test node summary");
    assert_eq!(retrieved.node_type, NodeType::Axiom);
}

#[test]
fn via_connects_layers() {
    let mut graph = SessionGraph::new("test-session");
    let via = Via {
        from_layer: LayerKind::Causal,
        from_node: NodeId("N1".to_string()),
        to_layer: LayerKind::Semantic,
        to_node: NodeId("S1".to_string()),
        via_type: ViaType::AnalogyOf,
        strength: 0.85,
    };
    graph.vias.push(via);
    assert_eq!(graph.vias.len(), 1);
    assert_eq!(graph.vias[0].from_layer, LayerKind::Causal);
    assert_eq!(graph.vias[0].to_layer, LayerKind::Semantic);
    assert_eq!(graph.vias[0].via_type, ViaType::AnalogyOf);
}

// ── serializer ───────────────────────────────────────────────────────────────

#[test]
fn serialize_is_deterministic() {
    let mut graph = SessionGraph::new("sess-1");
    for id_str in &["N3", "N1", "N2"] {
        let id = NodeId(id_str.to_string());
        graph.layers.causal.nodes.insert(
            id.clone(),
            Node {
                id,
                summary: format!("summary for {}", id_str),
                node_type: NodeType::Resolved,
                activation_weight: 0.5,
                epistemic_state: 0.8,
                farga_ref: None,
            },
        );
    }
    let first = graph.serialize();
    let second = graph.serialize();
    assert_eq!(first, second, "serialize() must be deterministic");
}

#[test]
fn serialize_frontier_nodes_listed() {
    let mut graph = SessionGraph::new("sess-frontier");
    let id = NodeId("N8".to_string());
    let node = Node {
        id: id.clone(),
        summary: "multi-layer graph + lazy loading".to_string(),
        node_type: NodeType::Frontier,
        activation_weight: 1.0,
        epistemic_state: 0.2,
        farga_ref: None,
    };
    graph.layers.causal.nodes.insert(id, node);
    let output = graph.serialize();
    assert!(
        output.contains("frontier: [N8]"),
        "frontier node N8 not listed in header.\nOutput:\n{}",
        output
    );
}

#[test]
fn serialize_active_vias_only() {
    let mut graph = SessionGraph::new("sess-vias");

    // Weak via — strength < 0.3, must be omitted
    graph.vias.push(Via {
        from_layer: LayerKind::Causal,
        from_node: NodeId("N1".to_string()),
        to_layer: LayerKind::Semantic,
        to_node: NodeId("S1".to_string()),
        via_type: ViaType::SimilarTo,
        strength: 0.2,
    });

    // Strong via — strength >= 0.3, must appear
    graph.vias.push(Via {
        from_layer: LayerKind::Causal,
        from_node: NodeId("N3".to_string()),
        to_layer: LayerKind::Semantic,
        to_node: NodeId("S3".to_string()),
        via_type: ViaType::AnalogyOf,
        strength: 0.85,
    });

    let output = graph.serialize();
    assert!(
        !output.contains("N1 →"),
        "weak via (strength 0.2) should be omitted from VIAS section.\nOutput:\n{}",
        output
    );
    assert!(
        output.contains("N3 →"),
        "strong via (strength 0.85) must appear in VIAS section.\nOutput:\n{}",
        output
    );
}

// ── retrieve ─────────────────────────────────────────────────────────────────

#[test]
fn retrieve_returns_scope_nodes() {
    let mut graph = SessionGraph::new("ret-scope");
    let (n1, node) = make_node("N1", "scope-node-summary", NodeType::Axiom, 0.9);
    graph.layers.causal.nodes.insert(n1.clone(), node);

    let output = graph.retrieve(&[n1], 1);
    assert!(
        output.contains("scope-node-summary"),
        "scope node must appear in retrieve output\n{}",
        output
    );
}

#[test]
fn retrieve_follows_one_hop_edges() {
    let mut graph = SessionGraph::new("ret-edges");
    let (n1, node1) = make_node("N1", "scope-node", NodeType::Axiom, 0.9);
    let (n2, node2) = make_node("N2", "causal-neighbor-summary", NodeType::Resolved, 0.9);
    graph.layers.causal.nodes.insert(n1.clone(), node1);
    graph.layers.causal.nodes.insert(n2, node2);
    graph.layers.causal.edges.push(make_edge("N1", "N2", EdgeType::LeadsTo));

    let output = graph.retrieve(&[n1], 1);
    assert!(
        output.contains("causal-neighbor-summary"),
        "causal edge neighbor must appear in retrieve output\n{}",
        output
    );
}

#[test]
fn retrieve_follows_vias() {
    let mut graph = SessionGraph::new("ret-vias");
    let (n1, node1) = make_node("N1", "scope-node", NodeType::Axiom, 0.9);
    // S1 in semantic layer; epistemic_state < 0.8 so it shows in EPISTEMIC section
    let (s1, node_s1) = make_node("S1", "strong-via-target-summary", NodeType::Supporting, 0.5);
    graph.layers.causal.nodes.insert(n1.clone(), node1);
    graph.layers.semantic.nodes.insert(s1, node_s1);
    graph.vias.push(make_via("N1", "S1", 0.8)); // strength > 0.5 → followed

    let output = graph.retrieve(&[n1], 1);
    assert!(
        output.contains("strong-via-target-summary"),
        "via target (strength 0.8 > 0.5) must appear in retrieve output\n{}",
        output
    );
}

#[test]
fn retrieve_omits_weak_vias() {
    let mut graph = SessionGraph::new("ret-weak-vias");
    let (n1, node1) = make_node("N1", "scope-node", NodeType::Axiom, 0.9);
    let (s2, node_s2) = make_node("S2", "weak-via-target-summary", NodeType::Supporting, 0.5);
    graph.layers.causal.nodes.insert(n1.clone(), node1);
    graph.layers.semantic.nodes.insert(s2, node_s2);
    // strength 0.2 — below both traversal threshold (0.5) and serialize threshold (0.3)
    graph.vias.push(make_via("N1", "S2", 0.2));

    let output = graph.retrieve(&[n1], 1);
    assert!(
        !output.contains("weak-via-target-summary"),
        "weak via target (strength 0.2) must NOT appear in retrieve output\n{}",
        output
    );
}

#[test]
fn retrieve_follows_second_hop_vias() {
    let mut graph = SessionGraph::new("ret-second-hop");
    let (n1, node1) = make_node("N1", "scope-node", NodeType::Axiom, 0.9);
    let (s1, node_s1) = make_node("S1", "first-hop-via-target", NodeType::Supporting, 0.5);
    let (s2, node_s2) = make_node("S2", "second-hop-via-target", NodeType::Supporting, 0.5);
    graph.layers.causal.nodes.insert(n1.clone(), node1);
    graph.layers.semantic.nodes.insert(s1, node_s1);
    graph.layers.semantic.nodes.insert(s2, node_s2);
    graph.vias.push(make_via("N1", "S1", 0.8)); // hop 1: scope → S1
    graph.vias.push(make_via("S1", "S2", 0.8)); // hop 2: S1 → S2

    let two_hop = graph.retrieve(&[n1.clone()], 2);
    assert!(
        two_hop.contains("second-hop-via-target"),
        "S2 must appear when via_hops == 2\n{}",
        two_hop
    );

    let one_hop = graph.retrieve(&[n1], 1);
    assert!(
        !one_hop.contains("second-hop-via-target"),
        "S2 must NOT appear when via_hops == 1\n{}",
        one_hop
    );
}

#[test]
fn retrieve_only_follows_outgoing_edges() {
    let mut graph = SessionGraph::new("ret-direction");
    let (n1, node1) = make_node("N1", "scope-node", NodeType::Axiom, 0.9);
    let (n2, node2) = make_node("N2", "predecessor-node", NodeType::Resolved, 0.9);
    graph.layers.causal.nodes.insert(n1.clone(), node1);
    graph.layers.causal.nodes.insert(n2, node2);
    // Edge points FROM N2 TO N1 — N1 is the target, not the source
    graph.layers.causal.edges.push(make_edge("N2", "N1", EdgeType::LeadsTo));

    let output = graph.retrieve(&[n1], 1);
    assert!(
        !output.contains("predecessor-node"),
        "incoming edge predecessor must NOT appear in retrieve output\n{}",
        output
    );
}

#[test]
fn retrieve_output_is_subset_of_full() {
    let mut graph = SessionGraph::new("ret-subset");
    let (n1, node1) = make_node("N1", "scope-node-only", NodeType::Axiom, 0.9);
    graph.layers.causal.nodes.insert(n1.clone(), node1);

    // Extra disconnected nodes — present in full, absent from retrieve
    for i in 2..=5 {
        let id = NodeId(format!("N{}", i));
        let node = Node {
            id: id.clone(),
            summary: format!("disconnected-node-{}", i),
            node_type: NodeType::Resolved,
            activation_weight: 0.5,
            epistemic_state: 0.9,
            farga_ref: None,
        };
        graph.layers.causal.nodes.insert(id, node);
    }

    let full = graph.serialize();
    let retrieved = graph.retrieve(&[n1], 1);
    assert!(
        retrieved.len() < full.len(),
        "retrieve output ({} chars) must be shorter than full serialize ({} chars)\nRetrieved:\n{}\nFull:\n{}",
        retrieved.len(),
        full.len(),
        retrieved,
        full
    );
}
