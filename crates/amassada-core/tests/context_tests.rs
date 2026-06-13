use amassada_core::context::ContextBuilder;
use amassada_core::types::{AgentId, TurnRecord};
use chrono::Utc;

#[test]
fn build_context_respects_window() {
    let mut builder = ContextBuilder::new(3);
    for i in 0..5 {
        builder.push_turn(TurnRecord {
            agent_id: AgentId::new("agent-a"),
            persona: "builder".into(),
            content: format!("turn {}", i),
            round: 1,
            turn_index: i,
            timestamp: Utc::now(),
            tokens_used: 100,
        });
    }
    let ctx = builder.build_for(
        &AgentId::new("agent-b"),
        vec![],       // whispers
        None,         // moderator envelope
    );
    // Should only include last 3 turns
    assert!(ctx.contains("turn 4"));
    assert!(ctx.contains("turn 3"));
    assert!(ctx.contains("turn 2"));
    assert!(!ctx.contains("turn 1"));
    assert!(!ctx.contains("turn 0"));
}
