use amassada_core::types::*;

#[test]
fn session_state_transitions() {
    let s = SessionState::Initializing;
    assert!(!s.is_terminal());
    assert!(SessionState::Complete.is_terminal());
    assert!(SessionState::Failed.is_terminal());
}

#[test]
fn agent_id_roundtrip() {
    let id = AgentId::new("moderator");
    assert_eq!(id.as_str(), "moderator");
}
