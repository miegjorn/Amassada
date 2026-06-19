use amassada_core::dispatch::{TurnRequest, build_system_prompt};

#[test]
fn turn_request_default_has_no_thinking_budget() {
    let req = TurnRequest {
        system_prompt: "You are helpful.".into(),
        context: "Hello".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 1000,
        thinking_budget: None,
    };
    assert!(req.thinking_budget.is_none());
}

#[test]
fn turn_request_can_set_thinking_budget() {
    let req = TurnRequest {
        system_prompt: "You are helpful.".into(),
        context: "Hello".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 1000,
        thinking_budget: Some(6000),
    };
    assert_eq!(req.thinking_budget, Some(6000));
}

#[test]
fn build_system_prompt_includes_persona_and_domain() {
    let prompt = build_system_prompt("platform-architect", "You design systems.", false);
    assert!(prompt.contains("platform-architect"));
    assert!(prompt.contains("You design systems."));
}

#[test]
fn build_system_prompt_moderator_includes_close_block() {
    let prompt = build_system_prompt("orchestrator", "You moderate.", true);
    assert!(prompt.contains("[CLOSE]"));
    assert!(prompt.contains("[INVITE:"));
}
