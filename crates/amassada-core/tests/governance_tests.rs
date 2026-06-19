use amassada_core::governance::{
    RiskFactors, RiskTier, RiskWeights, TierThresholds, compute_risk_score, GovernanceConfig,
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
