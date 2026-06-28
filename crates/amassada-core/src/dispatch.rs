use crate::error::{AmassadaError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
    pub thinking_budget: Option<u32>,
    /// API key to use. When None, falls back to ANTHROPIC_API_KEY env var.
    /// Set by callers that resolve credentials via Gardian.
    pub api_key: Option<String>,
    /// Optional graph subset (from SessionGraph::retrieve) injected as system[0].
    /// When Some, system becomes two cached blocks: graph ctx first, then persona.
    /// When None, single-block behavior is preserved.
    pub shared_context: Option<String>,
    /// MCP tool scopes granted to this turn's agent. Propagated verbatim into
    /// `TurnHttpRequest` so the receiving agent pod can restrict its tool use.
    /// Empty means no scope restriction (org-level agents like Guilhem).
    pub mcp_scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TurnResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Returns the effective max_tokens for an API call, clamping upward when
/// extended thinking is active (budget > 0). Exported so tests can verify
/// the clamping logic directly.
pub fn effective_max_tokens(max_tokens: u32, thinking_budget: Option<u32>) -> u32 {
    match thinking_budget.filter(|&b| b > 0) {
        Some(budget) => max_tokens.max(budget + 1024),
        None => max_tokens,
    }
}

/// Build the JSON request body for the Anthropic Messages API.
///
/// Extracted so tests can verify body construction without making live API calls.
/// When `req.shared_context` is `Some(ctx)`, system becomes two cached blocks:
///   [0] graph context, [1] agent persona — both with `cache_control: ephemeral`.
/// When `None`, the single-block legacy behavior is preserved.
pub fn build_request_body(req: &TurnRequest) -> serde_json::Value {
    let max_tokens = effective_max_tokens(req.max_tokens, req.thinking_budget);

    // system as array-of-blocks so the API can cache the stable system prompt across
    // turns. The cache breakpoint fires when the prompt exceeds 1024 tokens (large
    // Fondament domain contexts); for smaller prompts the API ignores the hint silently.
    let system = match &req.shared_context {
        Some(ctx) => serde_json::json!([
            {"type": "text", "text": ctx,               "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": req.system_prompt, "cache_control": {"type": "ephemeral"}}
        ]),
        None => serde_json::json!([
            {"type": "text", "text": req.system_prompt, "cache_control": {"type": "ephemeral"}}
        ]),
    };

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": [{"role": "user", "content": req.context}]
    });

    if let Some(budget) = req.thinking_budget.filter(|&b| b > 0) {
        body["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": budget});
    }

    body
}

/// Calls the Anthropic Messages API directly via reqwest.
/// Uses the user-message-only format (no prefill, no multi-turn list).
pub async fn dispatch(req: TurnRequest) -> Result<TurnResponse> {
    let api_key = req.api_key.clone()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| AmassadaError::Dispatch("ANTHROPIC_API_KEY not set".into()))?;

    let body = build_request_body(&req);

    let client = reqwest::Client::new();
    let mut request_builder = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json");

    // prompt-caching beta is always on; interleaved-thinking only when budgeted.
    request_builder = request_builder.header("anthropic-beta", "prompt-caching-2024-07-31");
    if req.thinking_budget.filter(|&b| b > 0).is_some() {
        request_builder = request_builder.header("anthropic-beta", "interleaved-thinking-2025-05-14");
    }

    let resp = request_builder
        .json(&body)
        .send()
        .await
        .map_err(|e| AmassadaError::Dispatch(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AmassadaError::Dispatch(format!("API error {}: {}", status, text)));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| AmassadaError::Dispatch(e.to_string()))?;

    let text = json["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"].as_str() == Some("text")))
        .and_then(|b| b["text"].as_str())
        .unwrap_or("")
        .to_string();

    let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
    let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

    Ok(TurnResponse { text, input_tokens, output_tokens })
}

/// JSON body sent to an agent endpoint's POST /turn
#[derive(Debug, Serialize)]
pub struct TurnHttpRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
    /// MCP tool scopes granted to this turn. The receiving agent pod uses these
    /// to restrict which MCP tools it invokes. Empty means no restriction.
    pub mcp_scopes: Vec<String>,
}

/// JSON body returned from an agent endpoint's POST /turn
#[derive(Debug, Deserialize)]
pub struct TurnHttpResponse {
    pub text: String,
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
}

/// Dispatch a turn to an external agent endpoint instead of calling Anthropic directly.
/// The endpoint must accept POST /turn with TurnHttpRequest body, return TurnHttpResponse.
pub async fn dispatch_to_endpoint(endpoint_url: &str, req: TurnRequest) -> Result<TurnResponse> {
    let client = reqwest::Client::new();
    let turn_url = format!("{}/turn", endpoint_url.trim_end_matches('/'));
    let body = TurnHttpRequest {
        system_prompt: req.system_prompt.clone(),
        context: req.context.clone(),
        model: req.model.clone(),
        max_tokens: req.max_tokens,
        mcp_scopes: req.mcp_scopes.clone(),
    };
    let resp = client
        .post(&turn_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AmassadaError::Dispatch(format!("endpoint {}: {}", turn_url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AmassadaError::Dispatch(format!("endpoint {} returned {}: {}", turn_url, status, body)));
    }
    let turn_resp: TurnHttpResponse = resp.json().await
        .map_err(|e| AmassadaError::Dispatch(format!("endpoint response parse: {}", e)))?;
    Ok(TurnResponse {
        text: turn_resp.text,
        input_tokens: turn_resp.input_tokens,
        output_tokens: turn_resp.output_tokens,
    })
}

/// Build the system prompt for an agent.
pub fn build_system_prompt(
    persona: &str,
    domain_context: &str,
    is_moderator: bool,
) -> String {
    let block_syntax = if is_moderator {
        r#"## Block Syntax (Moderator)

You MUST structure responses using these block markers:

[CONSULT to: <agent-id>]
<question for private sidebar — resolved before [MAIN]>

[BTW to: <agent-id>|room]
<public side comment — visible in transcript>

[MAIN]
<your primary contribution this turn>

[LEAVE]
<optional — emit only if your contribution to this session is complete>

## Moderator-Only Blocks
[INVITE: <agent-id>]
[RELEASE: <agent-id>]
[FORK_CONSULTATION: <agent-a>, <agent-b>, <topic>]
[ADJUST_BUDGET: <pool>, <delta>]
[REQUEST_APPROVAL: <reason>]
[MODEL: <model-id> for: <agent-id>]
[CLOSE]
"#
    } else {
        r#"## Block Syntax

You MUST structure responses using these block markers:

[CONSULT to: <agent-id>]
<question for private sidebar — resolved before [MAIN]>

[BTW to: <agent-id>|room]
<public side comment — visible in transcript>

[MAIN]
<your primary contribution this turn>

[LEAVE]
<optional — emit only if your contribution to this session is complete>
"#
    };

    format!("You are a {persona} agent.\n\n{domain_context}\n\n{block_syntax}")
}
