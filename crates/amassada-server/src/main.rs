mod api;
mod ws;

use axum::{routing::{get, post}, Router};
use api::ServerState;
use amassada_core::types::SessionEvent;
use amassada_core::project_registry::ProjectRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let port = std::env::var("AMASSADA_PORT").unwrap_or("7700".into());
    let canvas_dir = std::env::var("AMASSADA_CANVAS_DIR")
        .unwrap_or("canvases/stdlib".into());
    let farga_url = std::env::var("FARGA_URL").ok();
    let fondament_path = std::env::var("FONDAMENT_PATH").unwrap_or("/fondament".into());
    let projects_path = std::env::var("AMASSADA_PROJECTS_PATH")
        .unwrap_or("config/projects.toml".into());

    let project_registry = {
        let path = std::path::Path::new(&projects_path);
        if path.exists() {
            ProjectRegistry::load(path)?
        } else {
            tracing::warn!("no project registry at {}, starting empty", projects_path);
            ProjectRegistry::default()
        }
    };

    let (event_tx, _) = broadcast::channel::<SessionEvent>(256);

    let state = ServerState {
        canvas_dir,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        event_tx,
        farga_url,
        fondament_path,
        project_registry: Arc::new(project_registry),
    };

    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .route("/sessions", post(api::start_session))
        .route("/sessions/:session_id/message", post(api::post_message))
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
