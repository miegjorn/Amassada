use axum::{extract::State, http::StatusCode, Json};
use amassada_core::types::SessionState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ServerState {
    pub canvas_dir: String,
    pub active_state: Arc<Mutex<SessionState>>,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub canvas_id: Option<String>,
    pub goal: String,
}

#[derive(Serialize)]
pub struct StartResponse {
    pub session_id: String,
    pub canvas_id: String,
}

pub async fn start_session(
    State(_s): State<ServerState>,
    Json(req): Json<StartRequest>,
) -> (StatusCode, Json<StartResponse>) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let canvas_id = req.canvas_id.unwrap_or("design-session".into());
    // Actual session start is async — client polls /state or connects to /ws
    (StatusCode::ACCEPTED, Json(StartResponse { session_id, canvas_id }))
}

pub async fn get_state(State(s): State<ServerState>) -> Json<String> {
    let state = s.active_state.lock().await;
    Json(format!("{:?}", *state))
}

#[derive(Deserialize)]
pub struct HumanInputReq {
    pub kind: String,
    pub content: String,
}

pub async fn post_human_input(
    State(_s): State<ServerState>,
    Json(req): Json<HumanInputReq>,
) -> StatusCode {
    tracing::info!("human input received: {} — {}", req.kind, req.content);
    StatusCode::ACCEPTED
}
