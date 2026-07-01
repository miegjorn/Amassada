# Thinking Budget Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `thinking_budget` from `ResolvedAgent` into the Anthropic API call in `amassada-core/src/dispatch.rs` so that agents with the `+aporia` modifier actually receive extended thinking at dispatch time.

**Architecture:** Add `thinking_budget: Option<u32>` to `TurnRequest`. When set, include `"thinking": {"type": "enabled", "budget_tokens": N}` in the request body and the required beta header. Fix response parsing to handle thinking blocks (content[0] may be a thinking block, not text).

**Tech Stack:** Rust, reqwest, serde_json. Amassada workspace at `/Users/bedardpl/project/Amassada`.

---

## Context

### Current `dispatch.rs`

```rust
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
}

pub async fn dispatch(req: TurnRequest) -> Result<TurnResponse> {
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
        .await ...

    // BUG: blindly takes content[0], breaks when thinking blocks precede text
    let text = json["content"][0]["text"].as_str().unwrap_or("").to_string();
}
```

### Anthropic extended thinking format

When `thinking_budget` is set:
- Add header: `anthropic-beta: interleaved-thinking-2025-05-14`
- Add to body: `"thinking": {"type": "enabled", "budget_tokens": N}`
- `max_tokens` must be greater than `budget_tokens` — ensure `max_tokens = max_tokens.max(budget_tokens + 1024)`
- Response `content` array may start with `{"type": "thinking", "thinking": "..."}` blocks before `{"type": "text", "text": "..."}` blocks
- Response parser must find the first block where `type == "text"` instead of blindly using `[0]`

### Baseline

```bash
cd /Users/bedardpl/project/Amassada && cargo test 2>&1 | grep "test result"
```

68 tests passing, 0 failures.

---

## Task 1: Wire thinking_budget into TurnRequest and dispatch

**Files:**
- Modify: `crates/amassada-core/src/dispatch.rs`
- Modify: `crates/amassada-core/tests/dispatch_tests.rs` (create if absent)

- [ ] **Step 1: Check if dispatch_tests.rs exists**

```bash
ls /Users/bedardpl/project/Amassada/crates/amassada-core/tests/
```

If absent, create it. If present, read it first.

- [ ] **Step 2: Write failing tests**

Create or append to `crates/amassada-core/tests/dispatch_tests.rs`:

```rust
use amassada_core::dispatch::{TurnRequest, build_system_prompt};

#[test]
fn turn_request_without_thinking_budget() {
    let req = TurnRequest {
        system_prompt: "You are helpful.".into(),
        context: "Hello".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 1000,
        thinking_budget: None,
    };
    assert!(req.thinking_budget.is_none());
}

#[test]
fn turn_request_with_thinking_budget() {
    let req = TurnRequest {
        system_prompt: "You are helpful.".into(),
        context: "Hello".into(),
        model: "claude-sonnet-4-6".into(),
        max_tokens: 1000,
        thinking_budget: Some(6000),
    };
    assert_eq!(req.thinking_budget, Some(6000));
}

#[test]
fn build_system_prompt_produces_nonempty_string() {
    let prompt = build_system_prompt("architect", "You design systems.", false);
    assert!(prompt.contains("architect"));
    assert!(prompt.contains("You design systems."));
}
```

- [ ] **Step 3: Run to verify failure**

```bash
cd /Users/bedardpl/project/Amassada && cargo test -p amassada-core dispatch 2>&1 | grep -E "^error|FAILED" | head -5
```

Expected: compile error — `thinking_budget` field doesn't exist on `TurnRequest`.

- [ ] **Step 4: Update dispatch.rs**

Replace the ENTIRE content of `crates/amassada-core/src/dispatch.rs`:

```rust
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone)]
pub struct TurnRequest {
    pub system_prompt: String,
    pub context: String,
    pub model: String,
    pub max_tokens: u32,
    /// When set, enables extended thinking on the API call.
    /// Callers should pass ResolvedAgent.thinking_budget here.
    pub thinking_budget: Option<u32>,
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

    // When extended thinking is active, max_tokens must exceed budget_tokens
    let max_tokens = if let Some(budget) = req.thinking_budget {
        req.max_tokens.max(budget + 1024)
    } else {
        req.max_tokens
    };

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": max_tokens,
        "system": req.system_prompt,
        "messages": [{"role": "user", "content": req.context}]
    });

    if let Some(budget) = req.thinking_budget {
        body["thinking"] = serde_json::json!({
            "type": "enabled",
            "budget_tokens": budget
        });
    }

    let mut request = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json");

    if req.thinking_budget.is_some() {
        request = request.header("anthropic-beta", "interleaved-thinking-2025-05-14");
    }

    let resp = request
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

    // Content array may contain thinking blocks before text blocks — find first text block
    let text = json["content"]
        .as_array()
        .and_then(|blocks| {
            blocks.iter().find(|b| b["type"].as_str() == Some("text"))
        })
        .and_then(|b| b["text"].as_str())
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
```

- [ ] **Step 5: Fix all callers of TurnRequest that now need `thinking_budget` field**

Search for all TurnRequest struct literals:

```bash
cd /Users/bedardpl/project/Amassada && grep -rn "TurnRequest {" --include="*.rs" | grep -v target
```

For each, add `thinking_budget: None` (or the appropriate value if it's in a context where `ResolvedAgent.thinking_budget` is available).

- [ ] **Step 6: Run all tests**

```bash
cd /Users/bedardpl/project/Amassada && cargo test 2>&1 | grep "test result"
```

Expected: 68 original + 3 new dispatch tests = 71 total, 0 failures.

- [ ] **Step 7: Commit**

```bash
cd /Users/bedardpl/project/Amassada && git add crates/amassada-core/src/dispatch.rs crates/amassada-core/tests/dispatch_tests.rs && git commit -m "feat: wire thinking_budget into dispatch — extended thinking support for aporia modifier"
```

---

## Self-Review

**Spec coverage:**
- ✓ `thinking_budget: Option<u32>` on `TurnRequest`
- ✓ Beta header only when thinking_budget is Some
- ✓ `max_tokens` clamped above `budget_tokens + 1024`
- ✓ Body includes `"thinking": {"type": "enabled", "budget_tokens": N}` when set
- ✓ Response parser finds first `type == "text"` block instead of blindly using `[0]`
- ✓ Tests cover with/without thinking_budget
