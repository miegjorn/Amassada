// WebSocket streaming for SessionEvent — stub for v0.1.0
// Real implementation: upgrade connection, subscribe to broadcast::Receiver,
// forward each SessionEvent as JSON. Wired in main.rs router.
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let _ = socket.recv().await; // satisfy unused variable warning
    tracing::info!("ws client connected");
    // In full impl: subscribe to MainSessionChannel broadcast, forward events
}
