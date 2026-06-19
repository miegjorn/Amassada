use amassada_core::dispatch::{TurnRequest, build_system_prompt, effective_max_tokens};

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
