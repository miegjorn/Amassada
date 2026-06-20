use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use crate::api::ServerState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: ServerState) {
    let mut rx = state.event_tx.subscribe();
    tracing::info!("ws client connected");
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        let json = match serde_json::to_string(&ev) {
                            Ok(j) => j,
                            Err(e) => { tracing::warn!("serialize error: {e}"); continue; }
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                        if matches!(ev, amassada_core::types::SessionEvent::SessionCompleted
                            | amassada_core::types::SessionEvent::SessionFailed { .. }) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("ws subscriber lagged, dropped {n} events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                // client ping or close frame
                match msg {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
    tracing::info!("ws client disconnected");
}
