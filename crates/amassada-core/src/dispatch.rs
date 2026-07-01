use crate::error::{AmassadaError, Result};
use fondament_core::types::StructuredReasoning;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
    /// Provider-agnostic reasoning capability request. Dispatch translates to
    /// Anthropic budget_tokens for claude/* models; gracefully dropped for
    /// Gemini/OpenAI-o (they reason natively) and endpoint-routed participants.
    pub structured_reasoning: Option<StructuredReasoning>,
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
    let thinking_budget = req.structured_reasoning.as_ref().map(|sr| sr.anthropic_budget());
    let max_tokens = effective_max_tokens(req.max_tokens, thinking_budget);

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

    if let Some(budget) = thinking_budget.filter(|&b| b > 0) {
        body["thinking"] = serde_json::json!({"type": "enabled", "budget_tokens": budget});
    }

    body
}

/// Calls the Anthropic Messages API directly via reqwest.
/// Uses the user-message-only format (no prefill, no multi-turn list).
///
/// For model-agnostic complementary support (Grok + Anthropic together):
/// Direct dispatch is only for Anthropic/Claude models. For grok* / xai* models,
/// participants must use the `endpoint` mechanism (see TurnRequest and canvas config).
/// This allows running different models for different stances/participants without
/// breaking the existing Anthropic path.
pub async fn dispatch(req: TurnRequest) -> Result<TurnResponse> {
    if !req.model.starts_with("claude") && !req.model.starts_with("anthropic") {
        return Err(AmassadaError::Dispatch(format!(
            "direct dispatch only supports anthropic/claude models (got '{}'); for grok/xai use endpoint in the participant/canvas config for complementary multi-model runs",
            req.model
        )));
    }

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

    // prompt-caching beta is always on; interleaved-thinking is added when thinking is budgeted.
    // Both flags must be sent in a single comma-separated anthropic-beta header — duplicate
    // header values are not supported by the Anthropic API gateway and will be rejected.
    let thinking_budget = req.structured_reasoning.as_ref().map(|sr| sr.anthropic_budget());
    let beta_header = if thinking_budget.filter(|&b| b > 0).is_some() {
        "prompt-caching-2024-07-31,interleaved-thinking-2025-05-14"
    } else {
        "prompt-caching-2024-07-31"
    };
    request_builder = request_builder.header("anthropic-beta", beta_header);

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

/// Build the system prompt for an agent turn.
///
/// Composes three layers into a single string:
/// 1. A persona declaration (`"You are a {persona} agent."`).
/// 2. The Fondament-resolved domain context body (`domain_context`), which may span
///    disciplines and stances from the extends chain.
/// 3. Block-protocol syntax instructions — the `[MAIN]`, `[CONSULT]`, `[BTW]`, and
///    `[LEAVE]` markers that structure every agent response, plus moderator-only blocks
///    (`[INVITE]`, `[RELEASE]`, `[CLOSE]`, etc.) when `is_moderator` is true.
///
/// The block markers are deliberately included in the system prompt. They are **not**
/// stripped here — that is Charradissa's responsibility before Matrix display.
/// Stripping them inside Amassada would break the block parser on the round return path.
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
