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
