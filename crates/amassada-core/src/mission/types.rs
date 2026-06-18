use serde::{Deserialize, Serialize};
use crate::types::OutputArtifact;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubObjectiveStatus {
    Pending,
    InProgress,
    Complete,
    OutOfScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubObjective {
    pub id: String,
    pub description: String,
    pub completion_condition: String,
    pub status: SubObjectiveStatus,
    pub output: Option<String>,
    pub last_eval_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionBudget {
    pub total_tokens: u64,
    pub discretionary: u64,
    pub discretionary_strategize_spent: u64,
    pub discretionary_evaluate_spent: u64,
    pub deployable: u64,
    pub deployable_spent: u64,
}

impl MissionBudget {
    pub fn new(total_tokens: u64) -> Self {
        let discretionary = total_tokens / 5;
        let deployable = total_tokens - discretionary;
        Self {
            total_tokens,
            discretionary,
            discretionary_strategize_spent: 0,
            discretionary_evaluate_spent: 0,
            deployable,
            deployable_spent: 0,
        }
    }

    pub fn discretionary_remaining(&self) -> u64 {
        let spent = self.discretionary_strategize_spent
            .saturating_add(self.discretionary_evaluate_spent);
        self.discretionary.saturating_sub(spent)
    }

    pub fn deployable_remaining(&self) -> u64 {
        self.deployable.saturating_sub(self.deployable_spent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPlan {
    pub canvas_id: String,
    pub sub_objective_ids: Vec<String>,
    pub budget_slice: u64,
    pub expected_artifact_description: String,
    pub prior_artifact_inject: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub satisfied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub canvas_id: String,
    pub budget_allocated: u64,
    pub budget_spent: u64,
    pub sub_objective_ids: Vec<String>,
    pub artifact: Option<String>,
    pub evaluation: Option<EvaluationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionMetadata {
    pub mission_id: String,
    pub goal: String,
    pub sessions_run: u32,
    pub canvas_types: Vec<String>,
    pub sub_objectives_completed: u32,
    pub total_tokens_spent: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FargaContribution {
    pub title: String,
    pub narrative: String,
    pub artifacts: Vec<OutputArtifact>,
    pub metadata: MissionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FargaVerdict {
    Submit { contribution: FargaContribution },
    Skip { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionOutcome {
    pub mission_id: String,
    pub exhausted: bool,
    pub completed_sub_objective_ids: Vec<String>,
    pub verdict: FargaVerdict,
    pub metadata: MissionMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_splits_20_80() {
        let b = MissionBudget::new(100_000);
        assert_eq!(b.discretionary, 20_000);
        assert_eq!(b.deployable, 80_000);
        assert_eq!(b.total_tokens, 100_000);
    }

    #[test]
    fn budget_remaining_tracks_spend() {
        let mut b = MissionBudget::new(100_000);
        b.discretionary_strategize_spent = 5_000;
        b.discretionary_evaluate_spent = 2_000;
        assert_eq!(b.discretionary_remaining(), 13_000);
        b.deployable_spent = 30_000;
        assert_eq!(b.deployable_remaining(), 50_000);
    }

    #[test]
    fn budget_remaining_saturates_at_zero() {
        let mut b = MissionBudget::new(10_000);
        b.deployable_spent = 99_000;
        assert_eq!(b.deployable_remaining(), 0);
    }
}
