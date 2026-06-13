use amassada_core::channels::{main_session::MainSessionChannel, whisper::WhisperQueue};
use amassada_core::types::{AgentId, WhisperMsg, SessionEvent};
use chrono::Utc;

#[tokio::test]
async fn main_session_broadcasts_to_subscribers() {
    let channel = MainSessionChannel::new(16);
    let mut rx = channel.subscribe();
    let event = SessionEvent::SessionStarted {
        canvas_id: "debate".into(),
        goal: "test goal".into(),
    };
    channel.publish(event.clone()).await.unwrap();
    let received = rx.recv().await.unwrap();
    assert!(matches!(received, SessionEvent::SessionStarted { .. }));
}

#[test]
fn whisper_queue_enqueues_and_drains() {
    let mut queue = WhisperQueue::new();
    let agent = AgentId::new("builder");
    let msg = WhisperMsg {
        from: AgentId::new("moderator"),
        content: "be concise".into(),
        timestamp: Utc::now(),
    };
    queue.enqueue(agent.clone(), msg);
    let drained = queue.drain(&agent);
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].content, "be concise");
    assert!(queue.drain(&agent).is_empty());
}
