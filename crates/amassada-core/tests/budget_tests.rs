use amassada_core::budget::{BudgetLedger, PoolName};

#[test]
fn charges_and_tracks_pool() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.charge(PoolName::MainSession, 10_000).unwrap();
    let state = b.state(PoolName::MainSession);
    assert_eq!(state.consumed, 10_000);
    assert_eq!(state.remaining, 70_000);
}

#[test]
fn rejects_charge_over_pool_limit() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    let result = b.charge(PoolName::MainSession, 90_000);
    assert!(result.is_err());
}

#[test]
fn rebalance_adjusts_pools() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.adjust(PoolName::MainSession, -10_000, PoolName::Consultations, 10_000).unwrap();
    let ms = b.state(PoolName::MainSession);
    let co = b.state(PoolName::Consultations);
    assert_eq!(ms.total, 70_000);
    assert_eq!(co.total, 25_000);
}

#[test]
fn pct_remaining_is_correct() {
    let mut b = BudgetLedger::new(100_000, 80_000, 15_000, 5_000);
    b.charge(PoolName::MainSession, 80_000).unwrap();
    let state = b.state(PoolName::MainSession);
    assert_eq!(state.pct_remaining, 0.0);
}
