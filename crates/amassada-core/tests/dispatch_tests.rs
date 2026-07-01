use amassada_core::dispatch::{TurnRequest, TurnHttpRequest, TurnHttpResponse, build_request_body, build_system_prompt, effective_max_tokens};

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
        mcp_scopes: vec![],
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
        mcp_scopes: vec![],
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

#[test]
fn turn_http_request_serializes_all_fields() {
    let req = TurnHttpRequest {
        system_prompt: "You are Guilhem.".into(),
        context: "What is the state of the stack?".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 4096,
        mcp_scopes: vec![],
    };
    let v = serde_json::to_value(&req).expect("TurnHttpRequest must serialize");
    assert_eq!(v["system_prompt"].as_str().unwrap(), "You are Guilhem.");
    assert_eq!(v["context"].as_str().unwrap(), "What is the state of the stack?");
    assert_eq!(v["model"].as_str().unwrap(), "claude-sonnet-4-6");
    assert_eq!(v["max_tokens"].as_u64().unwrap(), 4096);
}

#[test]
fn turn_http_request_carries_mcp_scopes() {
    let req = TurnHttpRequest {
        system_prompt: "You are a project agent.".into(),
        context: "What should I work on?".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 4096,
        mcp_scopes: vec!["farga:read".into(), "farga:write:alpha".into()],
    };
    let v = serde_json::to_value(&req).expect("must serialize");
    let scopes = v["mcp_scopes"].as_array().expect("mcp_scopes must be an array");
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes[0].as_str().unwrap(), "farga:read");
    assert_eq!(scopes[1].as_str().unwrap(), "farga:write:alpha");
}

#[test]
fn org_session_turn_has_empty_mcp_scopes() {
    // Guilhem / org sessions carry no scope restriction.
    let req = TurnHttpRequest {
        system_prompt: "You are Guilhem.".into(),
        context: "Hello.".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 4096,
        mcp_scopes: vec![],
    };
    let v = serde_json::to_value(&req).unwrap();
    let scopes = v["mcp_scopes"].as_array().unwrap();
    assert!(scopes.is_empty(), "org sessions must carry no MCP scope restriction");
}

#[test]
fn turn_http_response_deserializes_with_default_token_counts() {
    // input_tokens / output_tokens are #[serde(default)] — an endpoint may omit them.
    let json = r#"{"text":"the stack is converging"}"#;
    let resp: TurnHttpResponse = serde_json::from_str(json).expect("must deserialize without token counts");
    assert_eq!(resp.text, "the stack is converging");
    assert_eq!(resp.input_tokens, 0);
    assert_eq!(resp.output_tokens, 0);
}

#[test]
fn turn_http_response_deserializes_with_token_counts() {
    let json = r#"{"text":"hi","input_tokens":120,"output_tokens":35}"#;
    let resp: TurnHttpResponse = serde_json::from_str(json).expect("must deserialize with token counts");
    assert_eq!(resp.text, "hi");
    assert_eq!(resp.input_tokens, 120);
    assert_eq!(resp.output_tokens, 35);
}

/// Guard test for model-agnostic complementary support.
/// Non-claude models must be rejected on direct path so that callers use the
/// endpoint field on participants (per canvas) to route to Grok-capable agents.
/// This keeps the pure Anthropic dispatch path untouched.
#[tokio::test]
async fn direct_dispatch_rejects_grok_and_xai_models_for_complementary_use() {
    let req = TurnRequest {
        system_prompt: "test".into(),
        context: "hi".into(),
        model: "grok-3".into(),
        max_tokens: 100,
        thinking_budget: None,
        api_key: Some("dummy".into()),
        shared_context: None,
        mcp_scopes: vec![],
    };
    let err = amassada_core::dispatch::dispatch(req).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("direct dispatch only supports anthropic/claude"));
    assert!(msg.contains("grok/xai use endpoint"));
}

#[tokio::test]
async fn direct_dispatch_rejects_xai_prefixed_models() {
    let req = TurnRequest {
        system_prompt: "test".into(),
        context: "hi".into(),
        model: "xai:grok-beta".into(),
        max_tokens: 100,
        thinking_budget: None,
        api_key: Some("dummy".into()),
        shared_context: None,
        mcp_scopes: vec![],
    };
    let err = amassada_core::dispatch::dispatch(req).await.unwrap_err();
    assert!(err.to_string().contains("direct dispatch only supports"));
}
