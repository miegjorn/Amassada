use async_trait::async_trait;
use std::sync::Mutex;
use crate::error::Result;
use crate::mission::types::EvaluationResult;

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
}
