use serde::{Deserialize, Serialize};
use crate::governance::risk::{RiskScore, RiskTier};
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

// Stance slots per tier. "moderator" is always last.
// Non-moderator slots are filled project-first from involved_projects.
fn stance_slots(tier: &RiskTier) -> Vec<&'static str> {
    match tier {
        RiskTier::Low => vec!["realist", "realist", "moderator"],
        RiskTier::Medium => vec!["realist", "adversarial", "builder", "moderator"],
        RiskTier::High => vec!["adversarial", "adversarial", "realist", "moderator"],
        RiskTier::Critical => vec!["adversarial", "adversarial", "realist", "builder", "dreamer", "moderator"],
    }
}

fn counter_stances(tier: &RiskTier) -> Option<Vec<&'static str>> {
    match tier {
        RiskTier::High | RiskTier::Critical => Some(vec!["builder", "dreamer"]),
        _ => None,
    }
}

fn resolve_address(stance: &str, projects: &[String], project_idx: &mut usize) -> String {
    if stance == "moderator" {
        return "stances/moderator".into();
    }
    if *project_idx < projects.len() {
        let addr = format!("{}+{}", projects[*project_idx], stance);
        *project_idx += 1;
        addr
    } else {
        format!("stances/{}", stance)
    }
}

pub fn compose_session(
    risk_score: &RiskScore,
    involved_projects: &[String],
    config: &GovernanceConfig,
) -> SessionComposition {
    let primary_stances = stance_slots(&risk_score.tier);
    let mut project_idx = 0;
    let primary_session = primary_stances
        .iter()
        .map(|s| resolve_address(s, involved_projects, &mut project_idx))
        .collect();

    // Counter slots always use generic stances — not project-specific
    let counter_session = counter_stances(&risk_score.tier).map(|stances| {
        stances.iter().map(|s| format!("stances/{}", s)).collect()
    });

    let min_tokens = match risk_score.tier {
        RiskTier::Low => config.tier_minimums.low,
        RiskTier::Medium => config.tier_minimums.medium,
        RiskTier::High => config.tier_minimums.high,
        RiskTier::Critical => config.tier_minimums.critical,
    };
    let recommended_tokens = config.budget.per_session_cap.min(min_tokens * 2);

    SessionComposition {
        risk_score: risk_score.score,
        tier: risk_score.tier.clone(),
        primary_session,
        counter_session,
        budget: BudgetEnvelope { recommended_tokens, minimum_tokens: min_tokens },
        moderator_override: None,
    }
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
