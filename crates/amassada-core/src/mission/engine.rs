use std::collections::HashMap;
use uuid::Uuid;
use crate::mission::types::{MissionBudget, SubObjective};
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::{FargaVerdict, SubObjectiveStatus};
    use crate::mission::evaluator::MockEvaluator;
    use crate::mission::meta::MockMetaModerator;
    use crate::mission::session_runner::MockSessionRunner;

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
}
