use axum::{extract::{Path, State}, http::StatusCode, Json};
use amassada_core::types::{SessionState, SessionEvent};
use amassada_core::canvas::CanvasLibrary;
use amassada_core::dispatch::{self, TurnRequest, build_system_prompt};
use amassada_core::project_registry::ProjectRegistry;
use amassada_core::fondament::resolve_persona;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Default canvas used for the ongoing org-level conversation with Guilhem.
const ORG_SESSION_CANVAS: &str = "org-session";
const MAX_TOKENS_PER_TURN: u32 = 4096;

#[derive(Clone)]
pub struct ServerState {
    pub canvas_dir: String,
    pub sessions: Arc<tokio::sync::RwLock<HashMap<String, SessionHandle>>>,
    pub event_tx: broadcast::Sender<SessionEvent>,
    pub farga_url: Option<String>,
    /// Root of the Fondament checkout used to resolve participant domain context.
    pub fondament_path: String,
    pub project_registry: Arc<ProjectRegistry>,
}

pub struct SessionHandle {
    pub session_id: String,
    pub canvas_id: String,
    pub state: SessionState,
    pub last_activity: std::time::Instant,
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
    let _ = &req.goal;
    let session_id = uuid::Uuid::new_v4().to_string();
    let canvas_id = req.canvas_id.unwrap_or("design-session".into());
    // Actual session start is async — client polls /state or connects to /ws
    (StatusCode::ACCEPTED, Json(StartResponse { session_id, canvas_id }))
}

pub async fn get_state(State(s): State<ServerState>) -> Json<String> {
    let sessions = s.sessions.read().await;
    let summary: Vec<String> = sessions.values()
        .map(|h| format!("{}={:?}", h.session_id, h.state))
        .collect();
    Json(format!("[{}]", summary.join(", ")))
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

/// Accept an externally-produced `SessionEvent` and broadcast it to all WebSocket subscribers.
/// This is how external processes (Caissa, cursor capture, etc.) publish into the event bus.
pub async fn publish_event(
    State(s): State<ServerState>,
    Json(event): Json<SessionEvent>,
) -> StatusCode {
    match s.event_tx.send(event) {
        Ok(_) => StatusCode::ACCEPTED,
        Err(_) => {
            // No active subscribers — not an error, just nothing to receive it yet.
            StatusCode::ACCEPTED
        }
    }
}

#[derive(Deserialize)]
pub struct MessageRequest {
    pub content: String,
    pub sender: String,
    /// Matrix room id. When provided, Amassada looks up the project in the registry
    /// and resolves its Fondament persona instead of using the canvas participant's
    /// domain field — enabling multi-tenant dispatch from a single Amassada instance.
    pub room_id: Option<String>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub text: String,
    pub session_id: String,
}

/// Per-session message handler — the entry point for the agent-as-endpoint dispatch
/// pattern. Amassada prepares the context and POSTs to the participant's endpoint
/// (e.g. the Guilhem pod), which owns MCP tool access and returns the response.
///
/// MVP scope: load the canvas (defaulting to `org-session`), find the first
/// non-human participant that declares an `endpoint`, assemble its system prompt
/// from the Fondament-resolved domain context, and dispatch a single turn with the
/// human message injected as the user turn. Deeper SessionEngine integration
/// (graph persistence, multi-round, real Transport) follows.
pub async fn post_message(
    State(s): State<ServerState>,
    Path(session_id): Path<String>,
    Json(req): Json<MessageRequest>,
) -> (StatusCode, Json<MessageResponse>) {
    // 1. Load the canvas library from disk (picks up org-session.yaml at runtime).
    let library = match CanvasLibrary::from_stdlib_dir(std::path::PathBuf::from(&s.canvas_dir)) {
        Ok(lib) => lib,
        Err(e) => {
            return error_response(
                &session_id,
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("canvas library load failed: {}", e),
            );
        }
    };

    // 2. Ensure a session handle exists; default new sessions to the org-session canvas.
    let canvas_id = {
        let mut sessions = s.sessions.write().await;
        let handle = sessions.entry(session_id.clone()).or_insert_with(|| SessionHandle {
            session_id: session_id.clone(),
            canvas_id: ORG_SESSION_CANVAS.to_string(),
            state: SessionState::Running,
            last_activity: std::time::Instant::now(),
        });
        handle.last_activity = std::time::Instant::now();
        handle.canvas_id.clone()
    };

    let canvas = match library.get(&canvas_id).or_else(|| library.get(ORG_SESSION_CANVAS)) {
        Some(c) => c,
        None => {
            return error_response(
                &session_id,
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("canvas '{}' not found in {}", canvas_id, s.canvas_dir),
            );
        }
    };

    // 3. Find the first non-human participant with an endpoint set.
    let participant = match canvas.initial_participants.iter()
        .find(|p| !p.is_human() && p.has_endpoint())
    {
        Some(p) => p,
        None => {
            return error_response(
                &session_id,
                StatusCode::BAD_REQUEST,
                format!("canvas '{}' has no endpoint-backed participant", canvas_id),
            );
        }
    };
    let endpoint = participant.endpoint.clone().expect("has_endpoint guaranteed Some");

    // 4. Resolve the domain context from Fondament and assemble the system prompt.
    // When a room_id is provided, look up the project registry and load the persona
    // from the fondament_persona field — this is the multi-tenant path. Otherwise
    // fall back to the canvas participant's domain field (legacy / single-tenant).
    //
    // The project path also carries `mcp_scopes` from the registry entry so the
    // receiving agent pod can restrict its MCP tool use to the declared set.
    let (domain_context, mcp_scopes) = if let Some(ref room_id) = req.room_id {
        match s.project_registry.get_by_room(room_id) {
            Some(project) => {
                let ctx = match resolve_persona(&s.fondament_path, &project.fondament_persona) {
                    Ok(resolved) => resolved.context,
                    Err(e) => {
                        tracing::warn!(
                            "fondament persona resolution failed for room {} ({}): {}",
                            room_id, project.fondament_persona, e
                        );
                        // Tier-1 (DefinitionTree) lookup failed — fall back to tier-2:
                        // markdown scan across conventional file layouts under fondament_path.
                        // See fondament::resolve_persona for the full fallback chain description.
                        resolve_domain_context(&s.fondament_path, &project.fondament_persona)
                    }
                };
                (ctx, project.mcp_scopes.clone())
            }
            None => {
                // Not in registry: try fondament persona resolution (YAML definition)
                // before falling back to markdown-only domain context.
                match resolve_persona(&s.fondament_path, &participant.domain) {
                    Ok(resolved) => (resolved.context, vec![]),
                    Err(_) => {
                        tracing::warn!("room {} not in registry; fondament persona '{}' not found, using fallback", room_id, participant.domain);
                        (resolve_domain_context(&s.fondament_path, &participant.domain), vec![])
                    }
                }
            }
        }
    } else {
        // Org session (Guilhem) — no scope restriction.
        (resolve_domain_context(&s.fondament_path, &participant.domain), vec![])
    };
    let system_prompt = build_system_prompt(&participant.persona, &domain_context, participant.is_moderator());
    let model = participant.model.clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    // 5. Inject the human message as the user turn and dispatch to the endpoint.
    let turn_req = TurnRequest {
        system_prompt,
        context: format!("[{}]: {}", req.sender, req.content),
        model,
        max_tokens: MAX_TOKENS_PER_TURN,
        thinking_budget: participant.thinking_budget,
        api_key: None,
        shared_context: None,
        mcp_scopes,
    };

    match dispatch::dispatch_to_endpoint(&endpoint, turn_req).await {
        Ok(resp) => (StatusCode::OK, Json(MessageResponse { text: resp.text, session_id })),
        Err(e) => error_response(
            &session_id,
            StatusCode::BAD_GATEWAY,
            format!("endpoint dispatch failed: {}", e),
        ),
    }
}

fn error_response(session_id: &str, status: StatusCode, msg: String) -> (StatusCode, Json<MessageResponse>) {
    tracing::warn!("post_message error ({}): {}", session_id, msg);
    (status, Json(MessageResponse { text: msg, session_id: session_id.to_string() }))
}

/// Best-effort resolution of a participant's domain context from the Fondament
/// checkout. The `domain` is a path-like id (e.g. "fondament/guilhem"); we try a
/// handful of conventional file layouts and fall back to a generic descriptor when
/// none is present (the endpoint pod holds its own authoritative context regardless).
fn resolve_domain_context(fondament_path: &str, domain: &str) -> String {
    let base = std::path::Path::new(fondament_path);
    // A domain of "fondament/guilhem" rooted at /fondament should resolve under
    // /fondament/guilhem, so strip a leading "fondament/" segment if present.
    let rel = domain.strip_prefix("fondament/").unwrap_or(domain);

    let candidates = [
        base.join(format!("{}.md", rel)),
        base.join(rel).join("persona.md"),
        base.join(rel).join("README.md"),
        base.join(format!("{}.md", domain)),
        base.join(domain).join("persona.md"),
    ];

    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.trim().is_empty() {
                return content;
            }
        }
    }

    tracing::warn!("no Fondament domain context found for '{}' under {}", domain, fondament_path);
    format!("You operate in the {} domain.", domain)
}
