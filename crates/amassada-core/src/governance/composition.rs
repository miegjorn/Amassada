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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::config::{GovernanceConfig, GovernanceBudgetConfig, TierMinimums};
    use crate::governance::risk::{RiskScore, RiskTier, RiskWeights, TierThresholds};

    fn test_config() -> GovernanceConfig {
        GovernanceConfig {
            risk_weights: RiskWeights {
                primitive_proximity: 0.25,
                signal_concurrence: 0.20,
                signal_velocity: 0.15,
                reversibility: 0.20,
                impact: 0.15,
                precedent: 0.05,
            },
            tier_thresholds: TierThresholds { medium: 0.30, high: 0.55, critical: 0.80 },
            budget: GovernanceBudgetConfig {
                daily_tokens: 50_000,
                per_session_cap: 15_000,
                counter_session_cap: 10_000,
            },
            tier_minimums: TierMinimums { low: 2_000, medium: 5_000, high: 8_000, critical: 12_000 },
        }
    }

    fn risk_score(tier: RiskTier) -> RiskScore {
        let score = match tier {
            RiskTier::Low => 0.10,
            RiskTier::Medium => 0.40,
            RiskTier::High => 0.65,
            RiskTier::Critical => 0.90,
        };
        RiskScore { score, tier }
    }

    #[test]
    fn low_and_medium_have_no_counter_session() {
        let config = test_config();
        for tier in [RiskTier::Low, RiskTier::Medium] {
            let c = compose_session(&risk_score(tier), &[], &config);
            assert!(
                c.counter_session.is_none(),
                "{:?} tier must not require counter-session", c.tier
            );
            check_constitution(&c).expect("constitution check must pass for Low/Medium");
        }
    }

    #[test]
    fn high_and_critical_require_counter_session() {
        let config = test_config();
        for tier in [RiskTier::High, RiskTier::Critical] {
            let c = compose_session(&risk_score(tier.clone()), &[], &config);
            assert!(
                c.counter_session.is_some(),
                "{:?} tier must have counter-session", tier
            );
            check_constitution(&c).expect("constitution check must pass for High/Critical");
        }
    }

    #[test]
    fn check_constitution_rejects_missing_counter_session_for_high() {
        let composition = SessionComposition {
            risk_score: 0.65,
            tier: RiskTier::High,
            primary_session: vec!["stances/adversarial".into()],
            counter_session: None, // intentionally wrong
            budget: BudgetEnvelope { recommended_tokens: 8_000, minimum_tokens: 8_000 },
            moderator_override: None,
        };
        assert!(check_constitution(&composition).is_err());
    }

    #[test]
    fn slot_resolution_fills_from_involved_projects_first() {
        let config = test_config();
        let projects = vec!["auth-service".to_string(), "api-gateway".to_string()];
        let c = compose_session(&risk_score(RiskTier::High), &projects, &config);
        // High tier: ["adversarial", "adversarial", "realist", "moderator"]
        // First two adversarial slots → project+stance addresses
        assert!(c.primary_session[0].starts_with("auth-service+"), "slot 0 = auth-service+adversarial");
        assert!(c.primary_session[1].starts_with("api-gateway+"), "slot 1 = api-gateway+adversarial");
        // Third slot (realist) has no project left → generic stance
        assert!(c.primary_session[2].starts_with("stances/"), "slot 2 = generic stance");
    }

    #[test]
    fn slot_resolution_falls_back_to_generic_when_projects_exhausted() {
        let config = test_config();
        // No projects — all slots fall back to generic stances
        let c = compose_session(&risk_score(RiskTier::Medium), &[], &config);
        for addr in &c.primary_session {
            assert!(addr.starts_with("stances/"), "expected generic stance, got {}", addr);
        }
    }
}
