use amassada_core::transport::local::LocalTransport;
use amassada_core::transport::Transport;
use amassada_core::types::*;

// Object-safety compile check
fn _assert_object_safe(_: &dyn Transport) {}

#[tokio::test]
async fn local_transport_broadcasts_events() {
    let transport = LocalTransport::new_test();
    let event = SessionEvent::SessionStarted { canvas_id: "debate".into(), goal: "test".into() };
    transport.broadcast(&event).await.unwrap();
    // In test mode, events go to an internal buffer
    let events = transport.take_events();
    assert_eq!(events.len(), 1);
}

/// Integration test: LocalTransport::consult() makes a real Anthropic dispatch.
///
/// Requires ANTHROPIC_API_KEY in the environment.  Marked #[ignore] so CI does
/// not run it unless explicitly opted in with `cargo test -- --ignored`.
#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY; run manually with `cargo test -- --ignored`"]
async fn local_transport_consult_dispatches_and_returns_answer() {
    use amassada_core::channels::consult::ConsultRequest;

    let transport = LocalTransport::new_test();

    let req = ConsultRequest {
        requester: AgentId::new("moderator"),
        target: AgentId::new("analyst"),
        question: "What is 2 + 2? Reply with just the number.".into(),
        system_prompt: "You are an analyst agent. Reply concisely.".into(),
        model: "claude-haiku-4-5".into(),
    };

    let resp = transport.consult(&req).await.expect("consult must succeed with valid API key");

    assert_eq!(resp.from, AgentId::new("analyst"));
    assert!(!resp.content.is_empty(), "response content must be non-empty");
    assert!(resp.tokens_used > 0, "tokens_used must be positive");
}
