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
