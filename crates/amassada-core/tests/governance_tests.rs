use amassada_core::governance::{
    RiskFactors, RiskScore, RiskTier, RiskWeights, TierThresholds, compute_risk_score,
    GovernanceConfig, BudgetEnvelope, SessionComposition, check_constitution, compose_session,
    ConstitutionViolation, address_to_participant, compose_governance_canvas,
};

fn default_weights() -> RiskWeights {
    RiskWeights {
        primitive_proximity: 0.25,
        signal_concurrence: 0.20,
        signal_velocity: 0.15,
        reversibility: 0.20,
        impact: 0.15,
        precedent: 0.05,
    }
}

fn default_thresholds() -> TierThresholds {
    TierThresholds { medium: 0.30, high: 0.55, critical: 0.80 }
}

#[test]
fn low_risk_factors_produce_low_tier() {
    let factors = RiskFactors {
        primitive_proximity: 0.2,
        signal_concurrence: 0.1,
        signal_velocity: 0.1,
        reversibility: 0.0,
        impact: 0.1,
        precedent: 0.0,
        is_irreversible: false,
        is_org_wide: false,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert_eq!(result.tier, RiskTier::Low);
    assert!(result.score < 0.30, "score was {}", result.score);
}

#[test]
fn high_risk_factors_produce_high_tier() {
    let factors = RiskFactors {
        primitive_proximity: 0.8,
        signal_concurrence: 0.7,
        signal_velocity: 0.6,
        reversibility: 0.7,
        impact: 0.7,
        precedent: 0.5,
        is_irreversible: false,
        is_org_wide: false,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert_eq!(result.tier, RiskTier::High);
    assert!(result.score >= 0.55 && result.score < 0.80, "score was {}", result.score);
}

#[test]
fn org_wide_flag_forces_critical_regardless_of_score() {
    let factors = RiskFactors {
        primitive_proximity: 0.0,
        signal_concurrence: 0.0,
        signal_velocity: 0.0,
        reversibility: 0.0,
        impact: 0.0,
        precedent: 0.0,
        is_irreversible: false,
        is_org_wide: true,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert_eq!(result.tier, RiskTier::Critical);
    assert!(result.score >= 0.80, "score should be floored to critical threshold, was {}", result.score);
}

#[test]
fn irreversible_flag_forces_minimum_high_tier() {
    let factors = RiskFactors {
        primitive_proximity: 0.0,
        signal_concurrence: 0.0,
        signal_velocity: 0.0,
        reversibility: 0.0,
        impact: 0.0,
        precedent: 0.0,
        is_irreversible: true,
        is_org_wide: false,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert_eq!(result.tier, RiskTier::High);
    assert!(result.score >= 0.55, "score should be floored to high threshold, was {}", result.score);
}

#[test]
fn org_wide_wins_over_irreversible() {
    let factors = RiskFactors {
        primitive_proximity: 0.0,
        signal_concurrence: 0.0,
        signal_velocity: 0.0,
        reversibility: 0.0,
        impact: 0.0,
        precedent: 0.0,
        is_irreversible: true,
        is_org_wide: true,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert_eq!(result.tier, RiskTier::Critical);
}

#[test]
fn weighted_sum_matches_manual_calculation() {
    let factors = RiskFactors {
        primitive_proximity: 1.0,
        signal_concurrence: 1.0,
        signal_velocity: 1.0,
        reversibility: 1.0,
        impact: 1.0,
        precedent: 1.0,
        is_irreversible: false,
        is_org_wide: false,
    };
    let result = compute_risk_score(&factors, &default_weights(), &default_thresholds());
    assert!((result.score - 1.0).abs() < 0.001, "expected 1.0, got {}", result.score);
    assert_eq!(result.tier, RiskTier::Critical);
}

const SAMPLE_CONFIG: &str = r#"
governance:
  risk_weights:
    primitive_proximity: 0.25
    signal_concurrence: 0.20
    signal_velocity: 0.15
    reversibility: 0.20
    impact: 0.15
    precedent: 0.05
  tier_thresholds:
    medium: 0.30
    high: 0.55
    critical: 0.80
  budget:
    daily_tokens: 50000
    per_session_cap: 15000
    counter_session_cap: 10000
  tier_minimums:
    low: 2000
    medium: 5000
    high: 8000
    critical: 12000
"#;

#[test]
fn governance_config_parses_from_yaml() {
    let config = GovernanceConfig::from_yaml(SAMPLE_CONFIG).unwrap();
    assert!((config.risk_weights.primitive_proximity - 0.25).abs() < 0.001);
    assert!((config.risk_weights.precedent - 0.05).abs() < 0.001);
    assert!((config.tier_thresholds.high - 0.55).abs() < 0.001);
    assert_eq!(config.budget.per_session_cap, 15_000);
    assert_eq!(config.tier_minimums.critical, 12_000);
}

#[test]
fn governance_config_weights_sum_to_one() {
    let config = GovernanceConfig::from_yaml(SAMPLE_CONFIG).unwrap();
    let w = &config.risk_weights;
    let sum = w.primitive_proximity + w.signal_concurrence + w.signal_velocity
        + w.reversibility + w.impact + w.precedent;
    assert!((sum - 1.0).abs() < 0.001, "weights must sum to 1.0, got {}", sum);
}

#[test]
fn governance_config_from_yaml_rejects_empty_string() {
    let result = GovernanceConfig::from_yaml("");
    assert!(result.is_err(), "empty YAML should fail to parse");
}

// ── Task 3: compose_session tests ─────────────────────────────────────────────

fn default_config() -> GovernanceConfig {
    GovernanceConfig::default_weights()
}

fn risk_score(tier: RiskTier, score: f32) -> RiskScore {
    RiskScore { score, tier }
}

#[test]
fn low_tier_produces_no_counter_session() {
    let rs = risk_score(RiskTier::Low, 0.20);
    let comp = compose_session(&rs, &["auth".into()], &default_config());
    assert_eq!(comp.tier, RiskTier::Low);
    assert!(comp.counter_session.is_none());
    assert_eq!(comp.primary_session.len(), 3, "Low tier: realist, realist, moderator");
}

#[test]
fn medium_tier_produces_no_counter_session() {
    let rs = risk_score(RiskTier::Medium, 0.40);
    let comp = compose_session(&rs, &[], &default_config());
    assert_eq!(comp.tier, RiskTier::Medium);
    assert!(comp.counter_session.is_none());
    assert_eq!(comp.primary_session.len(), 4, "Medium tier: realist, adversarial, builder, moderator");
}

#[test]
fn high_tier_produces_counter_session() {
    let rs = risk_score(RiskTier::High, 0.65);
    let comp = compose_session(&rs, &[], &default_config());
    assert_eq!(comp.tier, RiskTier::High);
    assert!(comp.counter_session.is_some());
    let counter = comp.counter_session.as_ref().unwrap();
    assert_eq!(counter.len(), 2, "Counter: builder, dreamer");
}

#[test]
fn critical_tier_produces_counter_session() {
    let rs = risk_score(RiskTier::Critical, 0.90);
    let comp = compose_session(&rs, &[], &default_config());
    assert_eq!(comp.tier, RiskTier::Critical);
    assert!(comp.counter_session.is_some());
    assert_eq!(comp.primary_session.len(), 6, "Critical primary: adversarial, adversarial, realist, builder, dreamer, moderator");
}

#[test]
fn involved_projects_fill_non_moderator_slots_first() {
    let rs = risk_score(RiskTier::High, 0.65);
    let projects = vec!["auth-service".into(), "api-gateway".into()];
    let comp = compose_session(&rs, &projects, &default_config());
    assert!(comp.primary_session[0].contains("auth-service"), "first slot: {}", comp.primary_session[0]);
    assert!(comp.primary_session[1].contains("api-gateway"), "second slot: {}", comp.primary_session[1]);
    assert!(comp.primary_session[2].starts_with("stances/"), "third slot falls back to generic: {}", comp.primary_session[2]);
    assert_eq!(comp.primary_session.last().unwrap(), "stances/moderator");
}

#[test]
fn moderator_slot_always_generic() {
    let rs = risk_score(RiskTier::Critical, 0.85);
    let projects: Vec<String> = (0..10).map(|i| format!("proj-{}", i)).collect();
    let comp = compose_session(&rs, &projects, &default_config());
    assert_eq!(comp.primary_session.last().unwrap(), "stances/moderator");
}

#[test]
fn constitution_passes_for_high_with_counter() {
    let comp = SessionComposition {
        risk_score: 0.65,
        tier: RiskTier::High,
        primary_session: vec!["stances/adversarial".into()],
        counter_session: Some(vec!["stances/builder".into()]),
        budget: BudgetEnvelope { recommended_tokens: 8000, minimum_tokens: 8000 },
        moderator_override: None,
    };
    assert!(check_constitution(&comp).is_ok());
}

#[test]
fn constitution_fails_for_high_without_counter() {
    let comp = SessionComposition {
        risk_score: 0.65,
        tier: RiskTier::High,
        primary_session: vec!["stances/adversarial".into()],
        counter_session: None,
        budget: BudgetEnvelope { recommended_tokens: 8000, minimum_tokens: 8000 },
        moderator_override: None,
    };
    assert!(check_constitution(&comp).is_err());
}

#[test]
fn constitution_passes_for_low_without_counter() {
    let comp = SessionComposition {
        risk_score: 0.20,
        tier: RiskTier::Low,
        primary_session: vec!["stances/realist".into()],
        counter_session: None,
        budget: BudgetEnvelope { recommended_tokens: 2000, minimum_tokens: 2000 },
        moderator_override: None,
    };
    assert!(check_constitution(&comp).is_ok());
}

// ── Task 4: canvas load test ──────────────────────────────────────────────────

use amassada_core::canvas::Canvas;

#[test]
fn governance_deliberation_canvas_parses() {
    let yaml = include_str!("../../../canvases/stdlib/governance-deliberation.yaml");
    let canvas = Canvas::from_yaml(yaml).expect("governance-deliberation canvas must parse");
    assert_eq!(canvas.id, "governance-deliberation");
    assert!(canvas.human.slot, "governance sessions always have a human slot");
    assert!(!canvas.output.sections.is_empty(), "output must have sections");
    let has_moderator = canvas.initial_participants.iter().any(|p| p.is_moderator());
    assert!(has_moderator, "canvas must include a moderator participant");
}

// ── Task 1: room.rs tests ─────────────────────────────────────────────────────

fn governance_canvas() -> Canvas {
    Canvas::from_yaml(
        include_str!("../../../canvases/stdlib/governance-deliberation.yaml")
    ).unwrap()
}

fn make_composition_for_room(
    primary: Vec<String>,
    override_addr: Option<String>,
    recommended: u32,
    minimum: u32,
) -> SessionComposition {
    SessionComposition {
        risk_score: 0.5,
        tier: RiskTier::Medium,
        primary_session: primary,
        counter_session: None,
        budget: BudgetEnvelope { recommended_tokens: recommended, minimum_tokens: minimum },
        moderator_override: override_addr,
    }
}

#[test]
fn address_to_participant_parses_generic_stance() {
    let p = address_to_participant("stances/realist");
    assert_eq!(p.persona, "realist");
    assert_eq!(p.domain, "stances/realist");
    assert!(p.model.is_none());
    assert!(p.authority.is_none());
}

#[test]
fn address_to_participant_parses_project_specific() {
    let p = address_to_participant("auth-service+adversarial");
    assert_eq!(p.persona, "adversarial");
    assert_eq!(p.domain, "auth-service+adversarial");
}

#[test]
fn address_to_participant_parses_moderator_slot() {
    let p = address_to_participant("stances/moderator");
    assert!(p.is_moderator());
    assert_eq!(p.domain, "stances/moderator");
}

#[test]
fn compose_governance_canvas_replaces_participants() {
    let base = governance_canvas();
    let primary = vec![
        "stances/realist".into(),
        "stances/adversarial".into(),
        "stances/moderator".into(),
    ];
    let comp = make_composition_for_room(primary, None, 5000, 2000);
    let result = compose_governance_canvas(base, &comp);
    assert_eq!(result.initial_participants.len(), 3);
    assert_eq!(result.initial_participants[0].persona, "realist");
    assert_eq!(result.initial_participants[1].persona, "adversarial");
    assert!(result.initial_participants[2].is_moderator());
}

#[test]
fn compose_governance_canvas_scales_budget() {
    let base = governance_canvas(); // total_tokens = 15000
    let comp = make_composition_for_room(vec!["stances/moderator".into()], None, 5000, 2000);
    let result = compose_governance_canvas(base, &comp);
    assert_eq!(result.budget.total_tokens, 5000);
}

#[test]
fn compose_governance_canvas_applies_moderator_override() {
    let base = governance_canvas();
    let comp = make_composition_for_room(
        vec!["stances/realist".into(), "stances/moderator".into()],
        Some("special-projects+moderator".into()),
        5000,
        2000,
    );
    let result = compose_governance_canvas(base, &comp);
    let mod_p = result.initial_participants.iter().find(|p| p.is_moderator()).unwrap();
    assert_eq!(mod_p.domain, "special-projects+moderator");
    assert_eq!(mod_p.persona, "moderator");
}

#[test]
fn compose_governance_canvas_no_override_keeps_original_moderator_domain() {
    let base = governance_canvas();
    let comp = make_composition_for_room(
        vec!["stances/moderator".into()],
        None,
        5000,
        2000,
    );
    let result = compose_governance_canvas(base, &comp);
    let mod_p = result.initial_participants.iter().find(|p| p.is_moderator()).unwrap();
    assert_eq!(mod_p.domain, "stances/moderator");
}
