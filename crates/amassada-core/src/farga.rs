//! Farga persistence — SessionGraph load/save over HTTP.
//!
//! All calls are non-fatal: errors are logged with `tracing::warn!` and the
//! caller continues with a fallback value (empty graph on load, silent drop
//! on save).

use crate::graph::SessionGraph;

/// POST `graph` to Farga as a node with kind `SessionGraph`, keyed by
/// `session_id`.
///
/// Body sent: `{ "kind": "SessionGraph", "key": "<session_id>",
///               "content": "<JSON-encoded graph>" }`
///
/// On success the remote node is created or updated. On any error the
/// problem is logged and the function returns normally so the caller is
/// not interrupted.
pub async fn save_graph(base_url: &str, session_id: &str, graph: &SessionGraph) {
    let content = match serde_json::to_string(graph) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("farga: failed to serialize SessionGraph (non-fatal): {}", e);
            return;
        }
    };

    let body = serde_json::json!({
        "kind":    "SessionGraph",
        "key":     session_id,
        "content": content,
    });

    let url = format!("{}/nodes", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();

    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!("farga: saved SessionGraph for session {}", session_id);
        }
        Ok(resp) => {
            tracing::warn!(
                "farga: POST {} returned {} (non-fatal)",
                url,
                resp.status()
            );
        }
        Err(e) => {
            tracing::warn!("farga: POST {} failed: {} (non-fatal)", url, e);
        }
    }
}

/// GET a previously persisted `SessionGraph` for `session_id`.
///
/// URL: `GET {base_url}/nodes/{session_id}?kind=SessionGraph`
/// Expected success body: `{ "content": "<JSON-encoded graph>" }` (HTTP 200)
/// 404 → treated as "not found", returns `None`.
/// Any other error → logged, returns `None`.
pub async fn load_graph(base_url: &str, session_id: &str) -> Option<SessionGraph> {
    let url = format!(
        "{}/nodes/{}?kind=SessionGraph",
        base_url.trim_end_matches('/'),
        session_id
    );

    let client = reqwest::Client::new();
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("farga: GET {} failed: {} (non-fatal)", url, e);
            return None;
        }
    };

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::debug!("farga: no existing graph for session {} (first run)", session_id);
        return None;
    }

    if !resp.status().is_success() {
        tracing::warn!(
            "farga: GET {} returned {} (non-fatal)",
            url,
            resp.status()
        );
        return None;
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("farga: failed to parse GET response: {} (non-fatal)", e);
            return None;
        }
    };

    let content = match json["content"].as_str() {
        Some(s) => s.to_string(),
        None => {
            tracing::warn!("farga: GET response has no 'content' field (non-fatal)");
            return None;
        }
    };

    match serde_json::from_str::<SessionGraph>(&content) {
        Ok(g) => {
            tracing::debug!(
                "farga: loaded SessionGraph v{} for session {}",
                g.version,
                session_id
            );
            Some(g)
        }
        Err(e) => {
            tracing::warn!(
                "farga: failed to deserialize SessionGraph: {} (non-fatal)",
                e
            );
            None
        }
    }
}
