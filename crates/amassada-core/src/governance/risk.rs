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
