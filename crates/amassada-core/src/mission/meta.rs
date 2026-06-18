use async_trait::async_trait;
use std::sync::Mutex;
use crate::error::Result;
use crate::mission::types::{FargaVerdict, SessionPlan};

/// Returns (output, tokens_used) so MissionEngine can track discretionary spend.
#[async_trait]
pub trait MetaModerator: Send + Sync {
    async fn strategize(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn replan(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn complete(&self, envelope: &str, artifacts_summary: &str) -> Result<(FargaVerdict, u64)>;
}

pub struct MockMetaModerator {
    plan_queue: Mutex<Vec<Vec<SessionPlan>>>,
    verdict: FargaVerdict,
}

impl MockMetaModerator {
    pub fn new(plan_batches: Vec<Vec<SessionPlan>>, verdict: FargaVerdict) -> Self {
        Self {
            plan_queue: Mutex::new(plan_batches),
            verdict,
        }
    }
}

#[async_trait]
impl MetaModerator for MockMetaModerator {
    async fn strategize(&self, _envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let mut q = self.plan_queue.lock().unwrap();
        Ok((if q.is_empty() { vec![] } else { q.remove(0) }, 0))
    }

    async fn replan(&self, _envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let mut q = self.plan_queue.lock().unwrap();
        Ok((if q.is_empty() { vec![] } else { q.remove(0) }, 0))
    }

    async fn complete(&self, _envelope: &str, _artifacts: &str) -> Result<(FargaVerdict, u64)> {
        Ok((self.verdict.clone(), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::{FargaVerdict, SessionPlan};

    fn make_plan(canvas_id: &str) -> SessionPlan {
        SessionPlan {
            canvas_id: canvas_id.into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "trade-off analysis".into(),
            prior_artifact_inject: false,
        }
    }

    #[tokio::test]
    async fn mock_meta_returns_queued_plans() {
        let mock = MockMetaModerator::new(
            vec![vec![make_plan("debate")], vec![make_plan("design-session")]],
            FargaVerdict::Skip { reason: "ephemeral".into() },
        );
        let (plans, tokens) = mock.strategize("envelope").await.unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].canvas_id, "debate");
        assert_eq!(tokens, 0);
        let (plans2, _) = mock.replan("envelope").await.unwrap();
        assert_eq!(plans2[0].canvas_id, "design-session");
    }

    #[tokio::test]
    async fn mock_meta_returns_configured_verdict() {
        let mock = MockMetaModerator::new(
            vec![],
            FargaVerdict::Skip { reason: "ephemeral".into() },
        );
        let (verdict, _) = mock.complete("envelope", "artifacts").await.unwrap();
        assert!(matches!(verdict, FargaVerdict::Skip { .. }));
    }
}
