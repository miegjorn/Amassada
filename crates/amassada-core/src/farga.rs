//! Farga persistence — SessionGraph load/save over HTTP.
//!
//! Uses Farga's generic KV store (`PUT`/`GET /kv/*path`), namespaced under
//! `graphs/<session_id>`. Previously targeted a `/nodes` endpoint that was
//! never actually implemented on Farga's server (confirmed live: every call
//! 404'd) — every save/load was silently a no-op, since both are
//! non-fatal-by-design. That meant no SessionGraph had ever actually
//! persisted across a call boundary; `load_graph` always fell back to a
//! fresh empty graph. `graphs/` as a namespace also means
//! `GET /kv/graphs` lists every known graph's session_id, which a future
//! garbage-collection pass needs to enumerate what to sweep.
//!
//! All calls remain non-fatal: errors are logged with `tracing::warn!` and
//! the caller continues with a fallback value (empty graph on load, silent
//! drop on save).

use crate::graph::SessionGraph;

/// Percent-encode a session_id (Matrix room IDs contain `!` and `:`) for use
/// as a URL path segment. Minimal — only encodes the characters that would
/// otherwise break path parsing or be misinterpreted.
fn encode_path_segment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '!' | '#' | ':' | '/' | '?' | '&' | '=' | '+' | ' ' | '%' => {
                format!("%{:02X}", c as u32)
            }
            _ => c.to_string(),
        })
        .collect()
}

/// PUT the SessionGraph into Farga's KV store at `graphs/<session_id>`.
///
/// On any error the problem is logged and the function returns normally so
/// the caller is not interrupted.
pub async fn save_graph(base_url: &str, session_id: &str, graph: &SessionGraph) {
    let value = match serde_json::to_value(graph) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("farga: failed to serialize SessionGraph (non-fatal): {}", e);
            return;
        }
    };

    let url = format!(
        "{}/kv/graphs/{}",
        base_url.trim_end_matches('/'),
        encode_path_segment(session_id)
    );
    let body = serde_json::json!({ "value": value });
    let client = reqwest::Client::new();

    match client.put(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!("farga: saved SessionGraph for session {}", session_id);
        }
        Ok(resp) => {
            tracing::warn!(
                "farga: PUT {} returned {} (non-fatal)",
                url,
                resp.status()
            );
        }
        Err(e) => {
            tracing::warn!("farga: PUT {} failed: {} (non-fatal)", url, e);
        }
    }
}

/// GET a previously persisted `SessionGraph` for `session_id` from
/// `graphs/<session_id>` in Farga's KV store.
///
/// 404 → treated as "not found" (first run for this session), returns `None`.
/// Any other error → logged, returns `None`.
pub async fn load_graph(base_url: &str, session_id: &str) -> Option<SessionGraph> {
    let url = format!(
        "{}/kv/graphs/{}",
        base_url.trim_end_matches('/'),
        encode_path_segment(session_id)
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

    let value = match json.get("value") {
        Some(v) => v,
        None => {
            tracing::warn!("farga: GET response has no 'value' field (non-fatal)");
            return None;
        }
    };

    match serde_json::from_value::<SessionGraph>(value.clone()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_path_segment_escapes_matrix_room_id_chars() {
        let encoded = encode_path_segment("!abc123:occitane.guilhem");
        assert!(!encoded.contains('!'));
        assert!(!encoded.contains(':'));
        assert_eq!(encoded, "%21abc123%3Aoccitane.guilhem");
    }

    #[test]
    fn encode_path_segment_leaves_plain_ids_unchanged() {
        assert_eq!(encode_path_segment("session-uuid-1234"), "session-uuid-1234");
    }
}
