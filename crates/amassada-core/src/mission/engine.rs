use std::collections::HashMap;
use uuid::Uuid;
use crate::canvas::Canvas;
use crate::error::Result;
use crate::mission::types::{
    EvaluationResult, FargaVerdict, MissionBudget, MissionMetadata, MissionOutcome,
    SessionPlan, SessionRecord, SubObjective, SubObjectiveStatus,
};
use crate::mission::evaluator::CompletionEvaluator;
use crate::mission::meta::MetaModerator;
use crate::mission::session_runner::SessionRunner;

pub struct MissionEngine {
    pub mission_id: String,
    pub goal: String,
    pub completion_condition: String,
    pub sub_objectives: Vec<SubObjective>,
    pub budget: MissionBudget,
    pub sessions_run: Vec<crate::mission::types::SessionRecord>,
    pub(crate) replan_counts: HashMap<String, u32>,
    runner: Box<dyn SessionRunner>,
    evaluator: Box<dyn CompletionEvaluator>,
    meta: Box<dyn MetaModerator>,
}

impl MissionEngine {
    pub fn new(
        goal: String,
        completion_condition: String,
        sub_objectives: Vec<SubObjective>,
        total_budget_tokens: u64,
        runner: Box<dyn SessionRunner>,
        evaluator: Box<dyn CompletionEvaluator>,
        meta: Box<dyn MetaModerator>,
    ) -> Self {
        Self {
            mission_id: Uuid::new_v4().to_string(),
            goal,
            completion_condition,
            sub_objectives,
            budget: MissionBudget::new(total_budget_tokens),
            sessions_run: Vec::new(),
            replan_counts: HashMap::new(),
            runner,
            evaluator,
            meta,
        }
    }

    pub async fn run(
        &mut self,
        canvas_lookup: impl Fn(&str) -> Option<Canvas>,
    ) -> Result<MissionOutcome> {
        use crate::mission::envelope::{build_mission_envelope, EnvelopeState};
        use crate::mission::session_runner::scale_canvas_budget;
        use std::time::Instant;

        let started_at = Instant::now();

        // STRATEGIZING
        let envelope = build_mission_envelope(&EnvelopeState {
            goal: &self.goal,
            completion_condition: &self.completion_condition,
            sub_objectives: &self.sub_objectives,
            budget: &self.budget,
            sessions_run: &self.sessions_run,
        });
        let (mut pending_plans, tokens) = self.meta.strategize(&envelope).await?;
        self.budget.discretionary_strategize_spent += tokens;

        // LOOP
        while !pending_plans.is_empty() {
            let plan = pending_plans.remove(0);

            // Budget guard
            if plan.budget_slice > self.budget.deployable_remaining() {
                return self.build_outcome(
                    FargaVerdict::Skip {
                        reason: "budget exhausted before plan could run".into(),
                    },
                    true,
                    started_at.elapsed().as_secs(),
                );
            }

            // Resolve canvas
            let canvas = canvas_lookup(&plan.canvas_id)
                .ok_or_else(|| crate::error::AmassadaError::CanvasNotFound(plan.canvas_id.clone()))?;
            let canvas = scale_canvas_budget(canvas, plan.budget_slice);

            // Build session goal — with optional prior artifact injection
            let session_goal = if plan.prior_artifact_inject {
                match self.sessions_run.last().and_then(|r| r.artifact.as_ref()) {
                    Some(prior) => format!(
                        "PRIOR SESSION OUTPUT:\n{prior}\n\nYOUR GOAL:\n{}",
                        plan.expected_artifact_description
                    ),
                    None => plan.expected_artifact_description.clone(),
                }
            } else {
                plan.expected_artifact_description.clone()
            };

            // RUNNING
            let output = self.runner.run(canvas.clone(), session_goal).await?;
            let artifact_text = output.artifacts.iter()
                .map(|a| format!("# {}\n{}", a.title, a.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            let tokens_spent = output.total_tokens as u64;
            self.budget.deployable_spent += tokens_spent;

            // EVALUATING — check each targeted sub-objective
            let mut all_satisfied = true;
            let mut first_fail_reason = String::new();

            for obj_id in &plan.sub_objective_ids {
                if let Some(obj) = self.sub_objectives.iter_mut().find(|o| &o.id == obj_id) {
                    if obj.status == SubObjectiveStatus::Complete {
                        continue;
                    }
                    obj.status = SubObjectiveStatus::InProgress;
                    let eval = self.evaluator.check(&obj.completion_condition, &artifact_text).await?;
                    self.budget.discretionary_evaluate_spent += 50; // Haiku eval est.
                    obj.last_eval_reason = Some(eval.reason.clone());

                    if eval.satisfied {
                        obj.status = SubObjectiveStatus::Complete;
                        obj.output = Some(artifact_text.clone());
                    } else {
                        all_satisfied = false;
                        if first_fail_reason.is_empty() {
                            first_fail_reason = format!("{}: {}", obj_id, eval.reason);
                        }
                    }
                }
            }

            // Record session
            let eval_result = if all_satisfied {
                Some(EvaluationResult { satisfied: true, reason: "all sub-objectives met".into() })
            } else {
                Some(EvaluationResult { satisfied: false, reason: first_fail_reason.clone() })
            };
            self.sessions_run.push(SessionRecord {
                session_id: output.session_id.clone(),
                canvas_id: canvas.id.clone(),
                budget_allocated: plan.budget_slice,
                budget_spent: tokens_spent,
                sub_objective_ids: plan.sub_objective_ids.clone(),
                artifact: Some(artifact_text.clone()),
                evaluation: eval_result,
            });

            if !all_satisfied {
                // REPLANNING — stub for now (implemented in Task 11)
                let replan_result = self.do_replan(&first_fail_reason, &plan.sub_objective_ids).await?;
                pending_plans.splice(0..0, replan_result);
                continue;
            }
        }

        // MISSION COMPLETION CHECK
        let all_artifacts = self.sessions_run.iter()
            .enumerate()
            .filter_map(|(i, r)| r.artifact.as_ref().map(|a| format!("SESSION {} ({}):\n{}", i + 1, r.canvas_id, a)))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let mission_eval = self.evaluator.check(&self.completion_condition, &all_artifacts).await?;
        self.budget.discretionary_evaluate_spent += 50;

        if !mission_eval.satisfied {
            return self.build_outcome(
                FargaVerdict::Skip { reason: format!("mission condition not met: {}", mission_eval.reason) },
                true,
                started_at.elapsed().as_secs(),
            );
        }

        // COMPLETING
        let envelope = build_mission_envelope(&EnvelopeState {
            goal: &self.goal,
            completion_condition: &self.completion_condition,
            sub_objectives: &self.sub_objectives,
            budget: &self.budget,
            sessions_run: &self.sessions_run,
        });
        let (mut verdict, tokens) = self.meta.complete(&envelope, &all_artifacts).await?;
        self.budget.discretionary_strategize_spent += tokens;

        // Patch metadata into Submit verdict
        if let FargaVerdict::Submit { ref mut contribution } = verdict {
            let all_oa: Vec<_> = self.sessions_run.iter()
                .flat_map(|r| r.artifact.iter().map(|a| crate::types::OutputArtifact {
                    id: r.session_id.clone(),
                    title: r.canvas_id.clone(),
                    content: a.clone(),
                    required: false,
                }))
                .collect();
            contribution.artifacts = all_oa;
            contribution.metadata = self.build_metadata(started_at.elapsed().as_secs());
        }

        self.build_outcome(verdict, false, started_at.elapsed().as_secs())
    }

    fn build_metadata(&self, duration_secs: u64) -> MissionMetadata {
        MissionMetadata {
            mission_id: self.mission_id.clone(),
            goal: self.goal.clone(),
            sessions_run: self.sessions_run.len() as u32,
            canvas_types: self.sessions_run.iter().map(|r| r.canvas_id.clone()).collect(),
            sub_objectives_completed: self.sub_objectives.iter()
                .filter(|o| o.status == SubObjectiveStatus::Complete)
                .count() as u32,
            total_tokens_spent: self.budget.deployable_spent
                + self.budget.discretionary_strategize_spent
                + self.budget.discretionary_evaluate_spent,
            duration_secs,
        }
    }

    fn build_outcome(
        &self,
        verdict: FargaVerdict,
        exhausted: bool,
        duration_secs: u64,
    ) -> Result<MissionOutcome> {
        Ok(MissionOutcome {
            mission_id: self.mission_id.clone(),
            exhausted,
            completed_sub_objective_ids: self.sub_objectives.iter()
                .filter(|o| o.status == SubObjectiveStatus::Complete)
                .map(|o| o.id.clone())
                .collect(),
            verdict,
            metadata: self.build_metadata(duration_secs),
        })
    }

    async fn do_replan(
        &mut self,
        reason: &str,
        _failed_obj_ids: &[String],
    ) -> Result<Vec<SessionPlan>> {
        // Stub — implemented in Task 11
        let _ = reason;
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::mission::evaluator::MockEvaluator;
    use crate::mission::meta::MockMetaModerator;
    use crate::mission::session_runner::MockSessionRunner;
    use crate::mission::types::{EvaluationResult, FargaVerdict, SessionPlan};
    use crate::types::{OutputArtifact, SessionOutput};

    pub fn make_sub_obj(id: &str) -> SubObjective {
        SubObjective {
            id: id.into(),
            description: format!("{id} description"),
            completion_condition: format!("{id} is done"),
            status: SubObjectiveStatus::Pending,
            output: None,
            last_eval_reason: None,
        }
    }

    fn make_engine() -> MissionEngine {
        MissionEngine::new(
            "decide auth approach".into(),
            "one approach chosen with rationale".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(MockSessionRunner::new(vec![])),
            Box::new(MockEvaluator::new(vec![])),
            Box::new(MockMetaModerator::new(vec![], FargaVerdict::Skip { reason: "test".into() })),
        )
    }

    fn stub_canvas_yaml() -> &'static str {
        "id: debate\nversion: \"1\"\nmode: auto\nselector:\n  description: d\n  tags: []\n  examples: []\ninitial_participants: []\nbudget:\n  total_tokens: 10000\n  pools:\n    main_session: 8000\n    consultations: 1500\n    mod_whisper: 500\nconsultation:\n  max_turns: 3\n  min_response_tokens: 50\nrounds:\n  min: 1\n  max: 5\n  convergence_modifier: 0.8\n  context_window: 8192\nhuman:\n  slot: false\noutput:\n  format: markdown\n  sections: []"
    }

    fn stub_session_output(content: &str) -> SessionOutput {
        SessionOutput {
            session_id: "s1".into(),
            canvas_id: "debate".into(),
            goal: "test".into(),
            artifacts: vec![OutputArtifact {
                id: "a1".into(), title: "Result".into(),
                content: content.into(), required: true,
            }],
            total_tokens: 5_000,
        }
    }

    #[test]
    fn new_initializes_budget_correctly() {
        let engine = make_engine();
        assert_eq!(engine.budget.total_tokens, 100_000);
        assert_eq!(engine.budget.discretionary, 20_000);
        assert_eq!(engine.budget.deployable, 80_000);
        assert_eq!(engine.sessions_run.len(), 0);
        assert_eq!(engine.sub_objectives.len(), 1);
        assert!(engine.replan_counts.is_empty());
    }

    #[tokio::test]
    async fn happy_path_single_session_completes_mission() {
        let plan = SessionPlan {
            canvas_id: "debate".into(),
            sub_objective_ids: vec!["obj-1".into()],
            budget_slice: 10_000,
            expected_artifact_description: "pros/cons analysis".into(),
            prior_artifact_inject: false,
        };

        let runner = MockSessionRunner::new(vec![stub_session_output("JWT wins: stateless, scalable.")]);
        let evaluator = MockEvaluator::new(vec![
            EvaluationResult { satisfied: true, reason: "analysis present".into() }, // sub-obj check
            EvaluationResult { satisfied: true, reason: "mission condition met".into() }, // mission check
        ]);
        let meta = MockMetaModerator::new(
            vec![vec![plan]],
            FargaVerdict::Submit {
                contribution: crate::mission::types::FargaContribution {
                    title: "Auth decision".into(),
                    narrative: "We chose JWT.".into(),
                    artifacts: vec![],
                    metadata: MissionMetadata {
                        mission_id: String::new(), goal: String::new(),
                        sessions_run: 1, canvas_types: vec!["debate".into()],
                        sub_objectives_completed: 1, total_tokens_spent: 10_000, duration_secs: 0,
                    },
                }
            },
        );

        let mut engine = MissionEngine::new(
            "decide auth".into(),
            "one approach chosen".into(),
            vec![make_sub_obj("obj-1")],
            100_000,
            Box::new(runner),
            Box::new(evaluator),
            Box::new(meta),
        );

        let outcome = engine.run(|id| {
            if id == "debate" { Some(Canvas::from_yaml(stub_canvas_yaml()).unwrap()) }
            else { None }
        }).await.unwrap();

        assert!(!outcome.exhausted);
        assert_eq!(outcome.completed_sub_objective_ids, vec!["obj-1"]);
        assert!(matches!(outcome.verdict, FargaVerdict::Submit { .. }));
        assert_eq!(engine.sessions_run.len(), 1);
        assert_eq!(engine.budget.deployable_spent, 5_000);
    }
}
