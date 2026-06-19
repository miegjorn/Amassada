pub mod risk;
pub mod config;
pub mod composition;

pub use risk::{RiskFactors, RiskScore, RiskTier, RiskWeights, TierThresholds, compute_risk_score};
pub use config::GovernanceConfig;
pub use composition::{BudgetEnvelope, ConstitutionViolation, SessionComposition, check_constitution, compose_session};
