use serde::{Deserialize, Serialize};
use crate::governance::risk::{RiskWeights, TierThresholds};
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierMinimums {
    pub low: u32,
    pub medium: u32,
    pub high: u32,
    pub critical: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceBudgetConfig {
    pub daily_tokens: u32,
    pub per_session_cap: u32,
    pub counter_session_cap: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    pub risk_weights: RiskWeights,
    pub tier_thresholds: TierThresholds,
    pub budget: GovernanceBudgetConfig,
    pub tier_minimums: TierMinimums,
}

impl GovernanceConfig {
    pub fn from_yaml(_yaml: &str) -> Result<Self> {
        Err(AmassadaError::CanvasParse("not implemented".into()))
    }

    pub fn default_weights() -> Self {
        Self {
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
}
