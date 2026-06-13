use amassada_core::blocks::{parse_blocks, AgentBlock, ModeratorAction};

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
