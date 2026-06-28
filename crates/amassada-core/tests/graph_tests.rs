use amassada_core::graph::*;

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
