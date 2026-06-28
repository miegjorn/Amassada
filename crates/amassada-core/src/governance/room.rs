use crate::canvas::{Canvas, ParticipantDef};
use crate::governance::composition::SessionComposition;
use crate::mission::session_runner::scale_canvas_budget;

pub fn address_to_participant(addr: &str) -> ParticipantDef {
    let persona = if let Some(pos) = addr.rfind('+') {
        &addr[pos + 1..]
    } else if let Some(pos) = addr.rfind('/') {
        &addr[pos + 1..]
    } else {
        addr
    };
    ParticipantDef {
        persona: persona.to_string(),
        domain: addr.to_string(),
        model: None,
        authority: None,
        thinking_budget: None,
        modifiers: vec![],
        endpoint: None,
    }
}

pub fn compose_governance_canvas(mut base: Canvas, composition: &SessionComposition) -> Canvas {
    base.initial_participants = composition.primary_session
        .iter()
        .map(|addr| address_to_participant(addr))
        .collect();

    base = scale_canvas_budget(base, composition.budget.recommended_tokens as u64);

    if let Some(override_addr) = &composition.moderator_override {
        for p in &mut base.initial_participants {
            if p.is_moderator() {
                tracing::info!(
                    old_domain = %p.domain,
                    new_domain = %override_addr,
                    "governance moderator override applied"
                );
                p.domain = override_addr.clone();
            }
        }
    }

    base
}
