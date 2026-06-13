use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolName { MainSession, Consultations, ModWhisper }

impl std::fmt::Display for PoolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainSession => write!(f, "main_session"),
            Self::Consultations => write!(f, "consultations"),
            Self::ModWhisper => write!(f, "mod_whisper"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolState {
    pub total: u32,
    pub consumed: u32,
    pub remaining: u32,
    pub pct_remaining: f32,
}

#[derive(Debug, Clone)]
struct Pool {
    total: u32,
    consumed: u32,
}

impl Pool {
    fn new(total: u32) -> Self { Self { total, consumed: 0 } }
    fn remaining(&self) -> u32 { self.total.saturating_sub(self.consumed) }
    fn pct_remaining(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.remaining() as f32 / self.total as f32 }
    }
    fn state(&self) -> PoolState {
        PoolState {
            total: self.total,
            consumed: self.consumed,
            remaining: self.remaining(),
            pct_remaining: self.pct_remaining(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BudgetLedger {
    total: u32,
    main_session: Pool,
    consultations: Pool,
    mod_whisper: Pool,
}

impl BudgetLedger {
    pub fn new(total: u32, main: u32, consult: u32, whisper: u32) -> Self {
        Self {
            total,
            main_session: Pool::new(main),
            consultations: Pool::new(consult),
            mod_whisper: Pool::new(whisper),
        }
    }

    fn pool_mut(&mut self, name: PoolName) -> &mut Pool {
        match name {
            PoolName::MainSession => &mut self.main_session,
            PoolName::Consultations => &mut self.consultations,
            PoolName::ModWhisper => &mut self.mod_whisper,
        }
    }

    fn pool(&self, name: PoolName) -> &Pool {
        match name {
            PoolName::MainSession => &self.main_session,
            PoolName::Consultations => &self.consultations,
            PoolName::ModWhisper => &self.mod_whisper,
        }
    }

    pub fn charge(&mut self, pool: PoolName, tokens: u32) -> Result<()> {
        let p = self.pool(pool);
        if tokens > p.remaining() {
            return Err(AmassadaError::BudgetExhausted { pool: pool.to_string() });
        }
        self.pool_mut(pool).consumed += tokens;
        Ok(())
    }

    pub fn adjust(&mut self, from: PoolName, delta: i64, to: PoolName, to_delta: i64) -> Result<()> {
        let from_pool = self.pool(from);
        let new_from_total = from_pool.total as i64 + delta;
        if new_from_total < from_pool.consumed as i64 {
            return Err(AmassadaError::BudgetExhausted { pool: from.to_string() });
        }
        self.pool_mut(from).total = new_from_total as u32;
        let to_pool = self.pool(to);
        let new_to_total = to_pool.total as i64 + to_delta;
        self.pool_mut(to).total = new_to_total.max(to_pool.consumed as i64) as u32;
        Ok(())
    }

    pub fn state(&self, pool: PoolName) -> PoolState { self.pool(pool).state() }

    pub fn all_states(&self) -> (PoolState, PoolState, PoolState) {
        (self.main_session.state(), self.consultations.state(), self.mod_whisper.state())
    }

    pub fn should_warn(&self, pool: PoolName) -> Option<f32> {
        let pct = self.pool(pool).pct_remaining();
        if pct <= 0.10 { Some(pct) } else if pct <= 0.20 { Some(pct) } else { None }
    }
}
