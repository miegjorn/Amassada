use async_trait::async_trait;
use std::sync::Mutex;
use crate::dispatch::{dispatch, TurnRequest};
use crate::error::{AmassadaError, Result};
use crate::mission::types::{FargaContribution, FargaVerdict, MissionMetadata, SessionPlan};

/// Returns (output, tokens_used) so MissionEngine can track discretionary spend.
#[async_trait]
pub trait MetaModerator: Send + Sync {
    async fn strategize(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn replan(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)>;
    async fn complete(&self, envelope: &str, artifacts_summary: &str) -> Result<(FargaVerdict, u64)>;
}

const META_SYSTEM_STRATEGIZE: &str = "\
You are a mission strategist for a multi-agent session engine. \
You receive a mission envelope describing a goal, completion condition, sub-objectives, and budget.

Your job: produce a JSON array of session plans that will satisfy pending sub-objectives.

Each plan is a JSON object:
{
  \"canvas_id\": \"<one of: debate, design-session, code-review-council, architectural-design, planning>\",
  \"sub_objective_ids\": [\"<id>\", ...],
  \"budget_slice\": <tokens to allocate — must not exceed deployable remaining>,
  \"expected_artifact_description\": \"<what this session should produce>\",
  \"prior_artifact_inject\": <true|false — true injects the previous session artifact into this session goal>
}

Rules:
- You may plan 1 to N sessions. You do not need to plan the full pipeline now.
- Sum of budget_slice values must not exceed the deployable budget remaining shown in the envelope.
- Respond ONLY with a JSON array. No prose, no markdown code fences.";

const META_SYSTEM_REPLAN: &str = "\
You are a mission strategist. A session failed its evaluation — the failure reason is in the envelope.

Produce replacement session plans as a JSON array (same schema as before).
Options: retry same canvas with refined goal, swap canvas type, insert missing intermediate session, \
or return an empty array to mark the sub-objective as out-of-scope.

Respond ONLY with a JSON array. No prose, no markdown code fences.";

const META_SYSTEM_COMPLETE: &str = "\
You are a mission strategist completing a mission. Review the envelope and all produced artifacts.

Decide if this output is worth adding to collective memory (Farga). Skip if ephemeral or task-specific.

Respond ONLY with valid JSON (no markdown):
{
  \"verdict\": \"submit\" | \"skip\",
  \"title\": \"<short title, if submit>\",
  \"narrative\": \"<What was sought. How approached. What was produced. What was decided. Why it belongs in collective memory. — if submit>\",
  \"skip_reason\": \"<reason — if skip>\"
}";

fn parse_session_plans(text: &str) -> Result<Vec<SessionPlan>> {
    serde_json::from_str(text.trim())
        .map_err(|e| AmassadaError::Mission(format!("meta strategize parse: {e} | raw: {}", &text[..text.len().min(200)])))
}

fn parse_farga_verdict(text: &str) -> Result<FargaVerdict> {
    let val: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| AmassadaError::Mission(format!("meta complete parse: {e} | raw: {}", &text[..text.len().min(200)])))?;

    match val["verdict"].as_str() {
        Some("submit") => Ok(FargaVerdict::Submit {
            contribution: FargaContribution {
                title: val["title"].as_str().unwrap_or("Mission Complete").to_string(),
                narrative: val["narrative"].as_str().unwrap_or("").to_string(),
                artifacts: vec![],   // MissionEngine patches real artifacts in after
                metadata: MissionMetadata {
                    mission_id: String::new(),
                    goal: String::new(),
                    sessions_run: 0,
                    canvas_types: vec![],
                    sub_objectives_completed: 0,
                    total_tokens_spent: 0,
                    duration_secs: 0,
                },
            },
        }),
        _ => Ok(FargaVerdict::Skip {
            reason: val["skip_reason"].as_str().unwrap_or("meta chose to skip").to_string(),
        }),
    }
}

pub struct ClaudeMetaModerator {
    pub model: String,
}

impl Default for ClaudeMetaModerator {
    fn default() -> Self {
        Self { model: "claude-opus-4-8".into() }
    }
}

#[async_trait]
impl MetaModerator for ClaudeMetaModerator {
    async fn strategize(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_STRATEGIZE.into(),
            context: envelope.into(),
            model: self.model.clone(),
            max_tokens: 2048,
            thinking_budget: None,
            api_key: None,
            shared_context: None,
        mcp_scopes: vec![],
        }).await?;
        let plans = parse_session_plans(&resp.text)?;
        Ok((plans, (resp.input_tokens + resp.output_tokens) as u64))
    }

    async fn replan(&self, envelope: &str) -> Result<(Vec<SessionPlan>, u64)> {
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_REPLAN.into(),
            context: envelope.into(),
            model: self.model.clone(),
            max_tokens: 2048,
            thinking_budget: None,
            api_key: None,
            shared_context: None,
        mcp_scopes: vec![],
        }).await?;
        let plans = parse_session_plans(&resp.text)?;
        Ok((plans, (resp.input_tokens + resp.output_tokens) as u64))
    }

    async fn complete(&self, envelope: &str, artifacts_summary: &str) -> Result<(FargaVerdict, u64)> {
        let context = format!("{envelope}\n\nALL ARTIFACTS:\n{artifacts_summary}");
        let resp = dispatch(TurnRequest {
            system_prompt: META_SYSTEM_COMPLETE.into(),
            context,
            model: self.model.clone(),
            max_tokens: 4096,
            thinking_budget: None,
            api_key: None,
            shared_context: None,
        mcp_scopes: vec![],
        }).await?;
        let verdict = parse_farga_verdict(&resp.text)?;
        Ok((verdict, (resp.input_tokens + resp.output_tokens) as u64))
    }
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

    #[tokio::test]
    #[ignore]
    async fn claude_meta_strategize_smoke() {
        let meta = ClaudeMetaModerator::default();
        let envelope = "MISSION GOAL\nDecide on auth approach\n\nCOMPLETION CONDITION\nOne approach chosen with rationale\n\nSUB-OBJECTIVES\n  [ ] obj-1: Surface trade-offs\n\nBUDGET\n  deployable: 0 / 80000 tokens used\n  sessions run: 0\n";
        let (plans, tokens) = meta.strategize(envelope).await.unwrap();
        assert!(!plans.is_empty(), "expected at least one plan");
        assert!(tokens > 0);
    }
}
