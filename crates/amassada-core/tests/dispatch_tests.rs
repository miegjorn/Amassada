use amassada_core::dispatch::{TurnRequest, build_request_body, build_system_prompt, effective_max_tokens};

#[test]
fn effective_max_tokens_no_budget_returns_original() {
    assert_eq!(effective_max_tokens(4096, None), 4096);
}

#[test]
fn effective_max_tokens_budget_zero_treated_as_no_budget() {
    assert_eq!(effective_max_tokens(4096, Some(0)), 4096);
}

#[test]
fn effective_max_tokens_small_budget_clamps_to_budget_plus_1024() {
    // budget=6000, original max_tokens=4096 → needs 6000+1024=7024
    assert_eq!(effective_max_tokens(4096, Some(6000)), 7024);
}

#[test]
fn effective_max_tokens_large_max_tokens_wins() {
    // original max_tokens already exceeds budget+1024
    assert_eq!(effective_max_tokens(10000, Some(6000)), 10000);
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
    assert!(prompt.contains("[INVITE: <agent-id>]"));
}

#[test]
fn dispatch_without_shared_context_uses_single_system_block() {
    let req = TurnRequest {
        system_prompt: "You are a test agent.".into(),
        context: "Hello.".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 256,
        thinking_budget: None,
        api_key: None,
        shared_context: None,
    };
    let body = build_request_body(&req);
    let system = body["system"].as_array().expect("system must be an array");
    assert_eq!(system.len(), 1, "expected one system block when shared_context is None");
    assert_eq!(system[0]["text"].as_str().unwrap(), "You are a test agent.");
    assert_eq!(system[0]["cache_control"]["type"].as_str().unwrap(), "ephemeral");
}

#[test]
fn dispatch_with_shared_context_uses_two_system_blocks() {
    let req = TurnRequest {
        system_prompt: "You are a test agent.".into(),
        context: "Hello.".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 256,
        thinking_budget: None,
        api_key: None,
        shared_context: Some("Graph context goes here.".into()),
    };
    let body = build_request_body(&req);
    let system = body["system"].as_array().expect("system must be an array");
    assert_eq!(system.len(), 2, "expected two system blocks when shared_context is Some");
    // block 0: graph context
    assert_eq!(system[0]["text"].as_str().unwrap(), "Graph context goes here.");
    assert_eq!(system[0]["cache_control"]["type"].as_str().unwrap(), "ephemeral");
    // block 1: agent persona
    assert_eq!(system[1]["text"].as_str().unwrap(), "You are a test agent.");
    assert_eq!(system[1]["cache_control"]["type"].as_str().unwrap(), "ephemeral");
}
