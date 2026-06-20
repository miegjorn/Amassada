mod api;
mod ws;

use axum::{routing::{get, post}, Router};
use api::ServerState;
use amassada_core::types::{SessionState, SessionEvent};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let port = std::env::var("AMASSADA_PORT").unwrap_or("7700".into());
    let canvas_dir = std::env::var("AMASSADA_CANVAS_DIR")
        .unwrap_or("canvases/stdlib".into());

    let (event_tx, _) = broadcast::channel::<SessionEvent>(256);

    let state = ServerState {
        canvas_dir,
        active_state: Arc::new(Mutex::new(SessionState::Initializing)),
        event_tx,
    };

    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .route("/sessions", post(api::start_session))
        .route("/state", get(api::get_state))
        .route("/human_input", post(api::post_human_input))
        .route("/events", post(api::publish_event))
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("amassada-server listening on :{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}
