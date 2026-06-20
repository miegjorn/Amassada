use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskWeights {
    pub primitive_proximity: f32,
    pub signal_concurrence: f32,
    pub signal_velocity: f32,
    pub reversibility: f32,
    pub impact: f32,
    pub precedent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierThresholds {
    pub medium: f32,
    pub high: f32,
    pub critical: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactors {
    pub primitive_proximity: f32,
    pub signal_concurrence: f32,
    pub signal_velocity: f32,
    pub reversibility: f32,
    pub impact: f32,
    pub precedent: f32,
    /// True when LibrarianAssessment.reversibility == Irreversible → minimum High tier
    pub is_irreversible: bool,
    /// True when LibrarianAssessment.impact == OrgWide → minimum Critical tier
    pub is_org_wide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    pub score: f32,
    pub tier: RiskTier,
}

pub fn compute_risk_score(
    factors: &RiskFactors,
    weights: &RiskWeights,
    thresholds: &TierThresholds,
) -> RiskScore {
    let raw = factors.primitive_proximity * weights.primitive_proximity
        + factors.signal_concurrence * weights.signal_concurrence
        + factors.signal_velocity * weights.signal_velocity
        + factors.reversibility * weights.reversibility
        + factors.impact * weights.impact
        + factors.precedent * weights.precedent;

    // Apply hard floor overrides
    let score = if factors.is_org_wide {
        raw.max(thresholds.critical)
    } else if factors.is_irreversible {
        raw.max(thresholds.high)
    } else {
        raw
    };

    let tier = if score >= thresholds.critical {
        RiskTier::Critical
    } else if score >= thresholds.high {
        RiskTier::High
    } else if score >= thresholds.medium {
        RiskTier::Medium
    } else {
        RiskTier::Low
    };

    RiskScore { score, tier }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn zero_factors() -> RiskFactors {
        RiskFactors {
            primitive_proximity: 0.0,
            signal_concurrence: 0.0,
            signal_velocity: 0.0,
            reversibility: 0.0,
            impact: 0.0,
            precedent: 0.0,
            is_irreversible: false,
            is_org_wide: false,
        }
    }

    #[test]
    fn low_score_produces_low_tier() {
        let factors = zero_factors();
        let r = compute_risk_score(&factors, &default_weights(), &default_thresholds());
        assert!(r.score < 0.30, "zero factors should be below medium threshold");
        assert_eq!(r.tier, RiskTier::Low);
    }

    #[test]
    fn org_wide_floor_forces_critical_regardless_of_raw_score() {
        let mut factors = zero_factors();
        factors.is_org_wide = true;
        let r = compute_risk_score(&factors, &default_weights(), &default_thresholds());
        // Raw score is 0.0 but floor must lift to critical
        assert!(r.score >= 0.80);
        assert_eq!(r.tier, RiskTier::Critical);
    }

    #[test]
    fn irreversible_floor_forces_at_least_high_tier() {
        let mut factors = zero_factors();
        factors.is_irreversible = true;
        let r = compute_risk_score(&factors, &default_weights(), &default_thresholds());
        assert!(r.score >= 0.55);
        assert!(matches!(r.tier, RiskTier::High | RiskTier::Critical));
    }

    #[test]
    fn weighted_sum_computes_correctly() {
        let factors = RiskFactors {
            primitive_proximity: 1.0,
            signal_concurrence: 1.0,
            signal_velocity: 1.0,
            reversibility: 0.0,
            impact: 0.0,
            precedent: 0.0,
            is_irreversible: false,
            is_org_wide: false,
        };
        let weights = default_weights();
        let expected = 1.0 * 0.25 + 1.0 * 0.20 + 1.0 * 0.15;
        let r = compute_risk_score(&factors, &weights, &default_thresholds());
        assert!((r.score - expected).abs() < 0.001);
    }

    #[test]
    fn changing_weights_changes_tier_for_same_factors() {
        let factors = RiskFactors {
            primitive_proximity: 0.5,
            signal_concurrence: 0.0,
            signal_velocity: 0.0,
            reversibility: 0.5,
            impact: 0.0,
            precedent: 0.0,
            is_irreversible: false,
            is_org_wide: false,
        };
        let thresholds = default_thresholds();

        let light_weights = RiskWeights {
            primitive_proximity: 0.05,
            signal_concurrence: 0.05,
            signal_velocity: 0.05,
            reversibility: 0.05,
            impact: 0.05,
            precedent: 0.05,
        };
        let r_light = compute_risk_score(&factors, &light_weights, &thresholds);
        assert_eq!(r_light.tier, RiskTier::Low, "small weights → Low");

        let heavy_weights = RiskWeights {
            primitive_proximity: 0.50,
            signal_concurrence: 0.10,
            signal_velocity: 0.10,
            reversibility: 0.20,
            impact: 0.05,
            precedent: 0.05,
        };
        let r_heavy = compute_risk_score(&factors, &heavy_weights, &thresholds);
        assert!(
            matches!(r_heavy.tier, RiskTier::Medium | RiskTier::High | RiskTier::Critical),
            "heavy weights → at least Medium"
        );
    }
}
