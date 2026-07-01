use async_trait::async_trait;
use std::sync::Mutex;
use crate::error::Result;
use crate::mission::types::EvaluationResult;
use crate::dispatch::{dispatch, TurnRequest};
use crate::error::AmassadaError;

#[async_trait]
pub trait CompletionEvaluator: Send + Sync {
    async fn check(&self, condition: &str, artifact: &str) -> Result<EvaluationResult>;
}

pub struct MockEvaluator {
    responses: Mutex<Vec<EvaluationResult>>,
}

impl MockEvaluator {
    pub fn new(responses: Vec<EvaluationResult>) -> Self {
        Self { responses: Mutex::new(responses) }
    }
}

#[async_trait]
impl CompletionEvaluator for MockEvaluator {
    async fn check(&self, _condition: &str, _artifact: &str) -> Result<EvaluationResult> {
        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            Ok(EvaluationResult { satisfied: true, reason: "mock: default pass".into() })
        } else {
            Ok(queue.remove(0))
        }
    }
}

pub struct ClaudeEvaluator {
    pub model: String,
}

impl Default for ClaudeEvaluator {
    fn default() -> Self {
        Self { model: "claude-haiku-4-5-20251001".into() }
    }
}

#[async_trait]
impl CompletionEvaluator for ClaudeEvaluator {
    async fn check(&self, condition: &str, artifact: &str) -> Result<EvaluationResult> {
        let system = "You are a completion evaluator. \
            Judge whether the artifact satisfies the condition. \
            Respond ONLY with valid JSON, no markdown fences: \
            {\"satisfied\": bool, \"reason\": \"one sentence explaining why yes or why not yet\"}";

        let context = format!("CONDITION:\n{condition}\n\nARTIFACT:\n{artifact}");

        let resp = dispatch(TurnRequest {
            system_prompt: system.into(),
            context,
            model: self.model.clone(),
            max_tokens: 256,
            structured_reasoning: None,
            api_key: None,
            shared_context: None,
        mcp_scopes: vec![],
        }).await?;

        let val: serde_json::Value = serde_json::from_str(resp.text.trim())
            .map_err(|e| AmassadaError::Mission(format!("evaluator JSON parse: {e}")))?;

        Ok(EvaluationResult {
            satisfied: val["satisfied"].as_bool().unwrap_or(false),
            reason: val["reason"].as_str().unwrap_or("(no reason)").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_evaluator_returns_queued_results() {
        let mock = MockEvaluator::new(vec![
            EvaluationResult { satisfied: false, reason: "not done yet".into() },
            EvaluationResult { satisfied: true,  reason: "condition met".into() },
        ]);
        let r1 = mock.check("condition", "artifact").await.unwrap();
        assert!(!r1.satisfied);
        assert_eq!(r1.reason, "not done yet");
        let r2 = mock.check("condition", "artifact").await.unwrap();
        assert!(r2.satisfied);
    }

    #[tokio::test]
    async fn mock_evaluator_defaults_to_pass_when_empty() {
        let mock = MockEvaluator::new(vec![]);
        let r = mock.check("condition", "artifact").await.unwrap();
        assert!(r.satisfied);
    }

    #[tokio::test]
    #[ignore]
    async fn claude_evaluator_smoke() {
        let eval = ClaudeEvaluator::default();
        let result = eval.check(
            "the text mentions a chosen option by name",
            "After deliberation, we chose JWT tokens for their statelessness.",
        ).await.unwrap();
        assert!(result.satisfied, "reason: {}", result.reason);
    }
}
