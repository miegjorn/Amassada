use amassada_core::blocks::{parse_blocks, AgentBlock, ModeratorAction, ProposalOp};
use amassada_core::graph::{EdgeType, LayerKind, NodeId, NodeType, ViaType};

#[test]
fn parses_main_block() {
    let input = "[MAIN]\nHere is my contribution to the debate.";
    let blocks = parse_blocks(input, false);
    assert_eq!(blocks.agent_blocks.len(), 1);
    if let AgentBlock::Main { content } = &blocks.agent_blocks[0] {
        assert_eq!(content.trim(), "Here is my contribution to the debate.");
    } else { panic!("expected Main block"); }
}

#[test]
fn parses_btw_block() {
    let input = "[BTW to: builder]\nQuick question about the approach.";
    let blocks = parse_blocks(input, false);
    assert_eq!(blocks.agent_blocks.len(), 1);
    if let AgentBlock::Btw { to, content } = &blocks.agent_blocks[0] {
        assert_eq!(to, "builder");
        assert_eq!(content.trim(), "Quick question about the approach.");
    } else { panic!("expected BTW block"); }
}

#[test]
fn parses_consult_block() {
    let input = "[CONSULT to: breaker]\nWhat's the security concern here?";
    let blocks = parse_blocks(input, false);
    if let AgentBlock::Consult { to, content: _ } = &blocks.agent_blocks[0] {
        assert_eq!(to, "breaker");
    } else { panic!("expected Consult block"); }
}

#[test]
fn parses_moderator_close() {
    let input = "[MAIN]\nFinal synthesis.\n[CLOSE]";
    let blocks = parse_blocks(input, true); // is_moderator = true
    assert!(blocks.moderator_actions.contains(&ModeratorAction::Close));
}

#[test]
fn parses_moderator_invite() {
    let input = "[INVITE: security-expert]\n[MAIN]\nI'm inviting an expert.";
    let blocks = parse_blocks(input, true);
    assert!(blocks.moderator_actions.iter().any(|a| matches!(a, ModeratorAction::Invite { .. })));
}

#[test]
fn ignores_moderator_blocks_for_non_moderator() {
    let input = "[CLOSE]\n[MAIN]\nNormal response.";
    let blocks = parse_blocks(input, false);
    assert!(blocks.moderator_actions.is_empty());
}

// ── GRAPH_PROPOSAL tests ──────────────────────────────────────────────────────

#[test]
fn parse_graph_proposal_update() {
    let input = "[GRAPH_PROPOSAL]\nUPDATE N1: activation_weight=0.95, epistemic_state=0.80";
    let parsed = parse_blocks(input, false);
    assert_eq!(parsed.agent_blocks.len(), 1);
    if let AgentBlock::GraphProposal { ops } = &parsed.agent_blocks[0] {
        assert_eq!(ops.len(), 1);
        if let ProposalOp::UpdateNode { id, activation_weight, epistemic_state } = &ops[0] {
            assert_eq!(*id, NodeId("N1".to_string()));
            assert!(activation_weight.is_some_and(|w| (w - 0.95).abs() < 1e-6),
                "activation_weight should be 0.95");
            assert!(epistemic_state.is_some_and(|e| (e - 0.80).abs() < 1e-6),
                "epistemic_state should be 0.80");
        } else {
            panic!("expected UpdateNode op");
        }
    } else {
        panic!("expected GraphProposal block");
    }
}

#[test]
fn parse_graph_proposal_new_node() {
    let input = concat!(
        "[GRAPH_PROPOSAL]\n",
        "NEW node: id=N3, type=resolved, summary=\"graph enables context compression\", ",
        "layer=causal, activation_weight=0.8, epistemic_state=0.05",
    );
    let parsed = parse_blocks(input, false);
    assert_eq!(parsed.agent_blocks.len(), 1);
    if let AgentBlock::GraphProposal { ops } = &parsed.agent_blocks[0] {
        assert_eq!(ops.len(), 1);
        if let ProposalOp::NewNode { node } = &ops[0] {
            assert_eq!(node.id, NodeId("N3".to_string()));
            assert_eq!(node.node_type, NodeType::Resolved);
            assert_eq!(node.summary, "graph enables context compression");
            assert!((node.activation_weight - 0.8).abs() < 1e-6,
                "activation_weight should be 0.8");
            assert!((node.epistemic_state - 0.05).abs() < 1e-6,
                "epistemic_state should be 0.05");
        } else {
            panic!("expected NewNode op");
        }
    } else {
        panic!("expected GraphProposal block");
    }
}

#[test]
fn parse_graph_proposal_new_edge() {
    let input = "[GRAPH_PROPOSAL]\nEDGE N1->N2: leads_to weight=1.0";
    let parsed = parse_blocks(input, false);
    assert_eq!(parsed.agent_blocks.len(), 1);
    if let AgentBlock::GraphProposal { ops } = &parsed.agent_blocks[0] {
        assert_eq!(ops.len(), 1);
        if let ProposalOp::NewEdge { edge } = &ops[0] {
            assert_eq!(edge.from, NodeId("N1".to_string()));
            assert_eq!(edge.to,   NodeId("N2".to_string()));
            assert_eq!(edge.edge_type, EdgeType::LeadsTo);
            assert!((edge.weight - 1.0).abs() < 1e-6, "weight should be 1.0");
        } else {
            panic!("expected NewEdge op");
        }
    } else {
        panic!("expected GraphProposal block");
    }
}

#[test]
fn parse_graph_proposal_new_via() {
    let input = "[GRAPH_PROPOSAL]\nVIA causal:N1->semantic:S1: analogy_of strength=0.70";
    let parsed = parse_blocks(input, false);
    assert_eq!(parsed.agent_blocks.len(), 1);
    if let AgentBlock::GraphProposal { ops } = &parsed.agent_blocks[0] {
        assert_eq!(ops.len(), 1);
        if let ProposalOp::NewVia { via } = &ops[0] {
            assert_eq!(via.from_layer, LayerKind::Causal);
            assert_eq!(via.from_node,  NodeId("N1".to_string()));
            assert_eq!(via.to_layer,   LayerKind::Semantic);
            assert_eq!(via.to_node,    NodeId("S1".to_string()));
            assert_eq!(via.via_type,   ViaType::AnalogyOf);
            assert!((via.strength - 0.70).abs() < 1e-6, "strength should be 0.70");
        } else {
            panic!("expected NewVia op");
        }
    } else {
        panic!("expected GraphProposal block");
    }
}

#[test]
fn parse_graph_proposal_mixed() {
    let input = concat!(
        "[GRAPH_PROPOSAL]\n",
        "UPDATE N1: activation_weight=0.95, epistemic_state=0.80\n",
        "NEW node: id=N5, type=frontier, summary=\"new idea to explore\", ",
            "layer=causal, activation_weight=0.6, epistemic_state=0.3\n",
        "EDGE N1->N5: leads_to weight=0.9\n",
        "VIA causal:N1->epistemic:E1: grounds strength=0.50",
    );
    let parsed = parse_blocks(input, false);
    assert_eq!(parsed.agent_blocks.len(), 1);
    if let AgentBlock::GraphProposal { ops } = &parsed.agent_blocks[0] {
        assert_eq!(ops.len(), 4, "expected 4 ops in mixed proposal");
        assert!(matches!(ops[0], ProposalOp::UpdateNode { .. }), "op[0] should be UpdateNode");
        assert!(matches!(ops[1], ProposalOp::NewNode { .. }),    "op[1] should be NewNode");
        assert!(matches!(ops[2], ProposalOp::NewEdge { .. }),    "op[2] should be NewEdge");
        assert!(matches!(ops[3], ProposalOp::NewVia { .. }),     "op[3] should be NewVia");
    } else {
        panic!("expected GraphProposal block");
    }
}
