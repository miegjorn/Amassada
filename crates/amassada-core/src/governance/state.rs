use crate::canvas::Canvas;
use crate::governance::composition::SessionComposition;
use crate::governance::config::GovernanceConfig;
use crate::governance::room::{address_to_participant, compose_governance_canvas};
use crate::mission::session_runner::scale_canvas_budget;
use crate::mission::types::MissionBudget;

pub struct GovernanceRoomSet {
    pub primary: Canvas,
    pub counter: Option<Canvas>,
}

pub enum GovernanceSessionState {
    PendingBudget {
        composition: SessionComposition,
        shortfall: u32,
    },
    Active {
        composition: SessionComposition,
        rooms: GovernanceRoomSet,
    },
}

pub fn init_governance_state(
    composition: SessionComposition,
    mission_budget: &MissionBudget,
    base_canvas: Canvas,
    config: &GovernanceConfig,
) -> GovernanceSessionState {
    let minimum = composition.budget.minimum_tokens as u64;
    let remaining = mission_budget.deployable_remaining();

    if remaining < minimum {
        let shortfall = (minimum - remaining).min(u32::MAX as u64) as u32;
        return GovernanceSessionState::PendingBudget { composition, shortfall };
    }

    // Clone counter addresses before compose_governance_canvas consumes base_canvas
    let counter_addrs = composition.counter_session.clone();

    let primary = compose_governance_canvas(base_canvas.clone(), &composition);

    let counter = counter_addrs.map(|addrs| {
        let mut c = base_canvas.clone();
        c.initial_participants = addrs.iter().map(|a| address_to_participant(a)).collect();
        scale_canvas_budget(c, config.budget.counter_session_cap as u64)
    });

    GovernanceSessionState::Active {
        composition,
        rooms: GovernanceRoomSet { primary, counter },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;
    use crate::governance::composition::{BudgetEnvelope, SessionComposition};
    use crate::governance::config::{GovernanceConfig, GovernanceBudgetConfig, TierMinimums};
    use crate::governance::risk::{RiskTier, RiskWeights, TierThresholds};
    use crate::mission::types::MissionBudget;

    fn test_canvas() -> Canvas {
        Canvas::from_yaml(
            r#"id: test
version: "1.0"
mode: auto
selector:
  description: "test governance canvas"
  tags: [test]
  examples: []
initial_participants: []
budget:
  total_tokens: 5000
  pools:
    main_session: 4000
    consultations: 800
    mod_whisper: 200
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 2
  max: 4
  convergence_modifier: 0.7
  context_window: 10
human:
  slot: false
output:
  format: markdown
  sections: []
"#,
        )
        .expect("test canvas parse failed")
    }

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

    fn composition_with_minimum(minimum: u32) -> SessionComposition {
        SessionComposition {
            risk_score: 0.40,
            tier: RiskTier::Medium,
            primary_session: vec!["stances/realist".into(), "stances/moderator".into()],
            counter_session: None,
            budget: BudgetEnvelope { recommended_tokens: minimum * 2, minimum_tokens: minimum },
            moderator_override: None,
        }
    }

    #[test]
    fn sufficient_budget_produces_active_state() {
        let composition = composition_with_minimum(5_000);
        let budget = MissionBudget::new(50_000);
        let state = init_governance_state(composition, &budget, test_canvas(), &test_config());
        assert!(matches!(state, GovernanceSessionState::Active { .. }));
    }

    #[test]
    fn insufficient_budget_produces_pending_budget_state() {
        let composition = composition_with_minimum(5_000);
        let budget = MissionBudget::new(1_000); // below minimum
        let state = init_governance_state(composition, &budget, test_canvas(), &test_config());
        match state {
            GovernanceSessionState::PendingBudget { shortfall, .. } => {
                assert!(shortfall > 0, "shortfall must be positive");
            }
            GovernanceSessionState::Active { .. } => {
                panic!("expected PendingBudget, got Active");
            }
        }
    }

    #[test]
    fn budget_exactly_at_minimum_produces_active_state() {
        // MissionBudget::deployable_remaining() = total * 4/5 (1/5 reserved as discretionary).
        // To get exactly 5_000 deployable, total = 6_250.
        let composition = composition_with_minimum(5_000);
        let budget = MissionBudget::new(6_250);
        let state = init_governance_state(composition, &budget, test_canvas(), &test_config());
        assert!(matches!(state, GovernanceSessionState::Active { .. }));
    }
}
