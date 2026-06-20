use crate::blocks::ModeratorAction;
use crate::budget::{BudgetLedger, PoolName};
use crate::channels::whisper::WhisperQueue;
use crate::types::{ActiveParticipant, AgentId, SessionEvent};

pub struct ModeratorExecutor;

pub struct ExecutionResult {
    pub should_close: bool,
    pub approval_requested: Option<String>,
    pub new_participants: Vec<AgentId>,
    pub released: Vec<AgentId>,
    pub events: Vec<SessionEvent>,
    pub canvas_switch: Option<String>,
    pub pending_forks: Vec<(String, String, String)>,
}

impl ModeratorExecutor {
    pub fn execute(
        &self,
        actions: Vec<ModeratorAction>,
        participants: &mut Vec<ActiveParticipant>,
        budget: &mut BudgetLedger,
        whisper_queue: &mut WhisperQueue,
    ) -> ExecutionResult {
        let _ = whisper_queue; // reserved for future whisper scheduling
        let mut result = ExecutionResult {
            should_close: false,
            approval_requested: None,
            new_participants: Vec::new(),
            released: Vec::new(),
            events: Vec::new(),
            canvas_switch: None,
            pending_forks: Vec::new(),
        };

        for action in actions {
            match action {
                ModeratorAction::Close => {
                    result.should_close = true;
                    result.events.push(SessionEvent::ModeratorAction { action: "CLOSE".into() });
                }
                ModeratorAction::Invite { agent_id } => {
                    let id = AgentId::new(&agent_id);
                    if !participants.iter().any(|p| p.agent_id == id) {
                        result.new_participants.push(id.clone());
                        result.events.push(SessionEvent::ModeratorAction {
                            action: format!("INVITE {}", agent_id)
                        });
                    }
                }
                ModeratorAction::Release { agent_id } => {
                    let id = AgentId::new(&agent_id);
                    result.released.push(id.clone());
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("RELEASE {}", agent_id)
                    });
                }
                ModeratorAction::RequestApproval { reason } => {
                    result.approval_requested = Some(reason.clone());
                    result.events.push(SessionEvent::ApprovalRequested { reason });
                }
                ModeratorAction::AdjustBudget { pool, delta } => {
                    let p = match pool.as_str() {
                        "main_session" => PoolName::MainSession,
                        "consultations" => PoolName::Consultations,
                        "mod_whisper" => PoolName::ModWhisper,
                        _ => continue,
                    };
                    let other = match pool.as_str() {
                        "main_session" => PoolName::Consultations,
                        _ => PoolName::MainSession,
                    };
                    let _ = budget.adjust(p, delta, other, -delta);
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("ADJUST_BUDGET {} {}", pool, delta)
                    });
                }
                ModeratorAction::SetModel { model, for_agent } => {
                    if let Some(p) = participants.iter_mut().find(|p| {
                        p.persona == for_agent || p.agent_id.to_string() == for_agent
                    }) {
                        p.model = Some(model.clone());
                    }
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("MODEL {} for {}", model, for_agent)
                    });
                }
                ModeratorAction::ForkConsultation { agent_a, agent_b, topic } => {
                    result.pending_forks.push((agent_a.clone(), agent_b.clone(), topic.clone()));
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("FORK_CONSULTATION {} {} {}", agent_a, agent_b, topic)
                    });
                }
                ModeratorAction::SwitchCanvas { canvas_id } => {
                    result.canvas_switch = Some(canvas_id.clone());
                    result.events.push(SessionEvent::ModeratorAction {
                        action: format!("SWITCH_CANVAS {}", canvas_id)
                    });
                }
            }
        }

        result
    }
}
