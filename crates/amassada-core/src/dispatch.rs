use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct TurnResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Calls the Anthropic Messages API directly via reqwest.
/// Uses the user-message-only format (no prefill, no multi-turn list).
pub async fn dispatch(req: TurnRequest) -> Result<TurnResponse> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| AmassadaError::Dispatch("ANTHROPIC_API_KEY not set".into()))?;

    let body = serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "system": req.system_prompt,
        "messages": [{"role": "user", "content": req.context}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
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

    let text = json["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
    let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

    Ok(TurnResponse { text, input_tokens, output_tokens })
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

    format!(
        "You are a {persona} agent.\n\n{domain_context}\n\n{block_syntax}",
        persona = persona,
        domain_context = domain_context,
        block_syntax = block_syntax,
    )
}
