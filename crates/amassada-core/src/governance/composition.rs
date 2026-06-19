use serde::{Deserialize, Serialize};
use crate::governance::risk::RiskTier;
use crate::governance::config::GovernanceConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEnvelope {
    pub recommended_tokens: u32,
    pub minimum_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionComposition {
    pub risk_score: f32,
    pub tier: RiskTier,
    pub primary_session: Vec<String>,
    pub counter_session: Option<Vec<String>>,
    pub budget: BudgetEnvelope,
    pub moderator_override: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConstitutionViolation {
    MissingCounterSession { tier: RiskTier },
}

impl std::fmt::Display for ConstitutionViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCounterSession { tier } => write!(
                f, "counter-session required for {:?} tier", tier
            ),
        }
    }
}

pub fn compose_session(
    _risk_score: &crate::governance::risk::RiskScore,
    _involved_projects: &[String],
    _config: &GovernanceConfig,
) -> SessionComposition {
    unimplemented!("Task 3")
}

pub fn check_constitution(composition: &SessionComposition) -> Result<(), ConstitutionViolation> {
    match &composition.tier {
        RiskTier::High | RiskTier::Critical => {
            if composition.counter_session.is_none() {
                return Err(ConstitutionViolation::MissingCounterSession {
                    tier: composition.tier.clone(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}
