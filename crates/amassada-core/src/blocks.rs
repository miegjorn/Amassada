use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentBlock {
    Main { content: String },
    Btw { to: String, content: String },
    Consult { to: String, content: String },
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeratorAction {
    Invite { agent_id: String },
    Release { agent_id: String },
    ForkConsultation { agent_a: String, agent_b: String, topic: String },
    AdjustBudget { pool: String, delta: i64 },
    RequestApproval { reason: String },
    SetModel { model: String, for_agent: String },
    Close,
    SwitchCanvas { canvas_id: String },
}

#[derive(Debug, Clone)]
pub struct ParsedResponse {
    pub agent_blocks: Vec<AgentBlock>,
    pub moderator_actions: Vec<ModeratorAction>,
    pub raw: String,
}

pub fn parse_blocks(input: &str, is_moderator: bool) -> ParsedResponse {
    let mut agent_blocks = Vec::new();
    let mut moderator_actions = Vec::new();

    // Split by block header patterns: lines starting with [BLOCK...]
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("[MAIN]") {
            // collect until next block header
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Main { content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[BTW to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Btw { to, content });
            i += 1;
        } else if let Some(to) = extract_param(line, "[CONSULT to:") {
            let content = collect_until_next_block(&lines, i + 1);
            agent_blocks.push(AgentBlock::Consult { to, content });
            i += 1;
        } else if line.starts_with("[LEAVE]") {
            agent_blocks.push(AgentBlock::Leave);
            i += 1;
        } else if is_moderator {
            if let Some(id) = extract_param(line, "[INVITE:") {
                moderator_actions.push(ModeratorAction::Invite { agent_id: id });
                i += 1;
            } else if let Some(id) = extract_param(line, "[RELEASE:") {
                moderator_actions.push(ModeratorAction::Release { agent_id: id });
                i += 1;
            } else if line.starts_with("[CLOSE]") {
                moderator_actions.push(ModeratorAction::Close);
                i += 1;
            } else if let Some(reason) = extract_param(line, "[REQUEST_APPROVAL:") {
                moderator_actions.push(ModeratorAction::RequestApproval { reason });
                i += 1;
            } else if line.starts_with("[ADJUST_BUDGET:") {
                // [ADJUST_BUDGET: main_session, -10000]
                let inner = line.trim_start_matches("[ADJUST_BUDGET:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let pool = parts[0].trim().to_string();
                    let delta: i64 = parts[1].trim().parse().unwrap_or(0);
                    moderator_actions.push(ModeratorAction::AdjustBudget { pool, delta });
                }
                i += 1;
            } else if line.starts_with("[MODEL:") {
                // [MODEL: claude-sonnet-4-6 for: builder]
                let inner = line.trim_start_matches("[MODEL:").trim_end_matches(']');
                if let Some((model, for_part)) = inner.split_once(" for:") {
                    moderator_actions.push(ModeratorAction::SetModel {
                        model: model.trim().to_string(),
                        for_agent: for_part.trim().to_string(),
                    });
                }
                i += 1;
            } else if line.starts_with("[FORK_CONSULTATION:") {
                let inner = line.trim_start_matches("[FORK_CONSULTATION:").trim_end_matches(']');
                let parts: Vec<&str> = inner.splitn(3, ',').collect();
                if parts.len() == 3 {
                    moderator_actions.push(ModeratorAction::ForkConsultation {
                        agent_a: parts[0].trim().to_string(),
                        agent_b: parts[1].trim().to_string(),
                        topic: parts[2].trim().to_string(),
                    });
                }
                i += 1;
            } else if line.starts_with("[SWITCH_CANVAS:") {
                let id = line.trim_start_matches("[SWITCH_CANVAS:").trim_end_matches(']').trim().to_string();
                moderator_actions.push(ModeratorAction::SwitchCanvas { canvas_id: id });
                i += 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    ParsedResponse { agent_blocks, moderator_actions, raw: input.to_string() }
}

fn extract_param(line: &str, prefix: &str) -> Option<String> {
    if line.starts_with(prefix) {
        let inner = line[prefix.len()..].trim_end_matches(']');
        Some(inner.trim().to_string())
    } else {
        None
    }
}

fn collect_until_next_block(lines: &[&str], start: usize) -> String {
    let block_starters = ["[MAIN]", "[BTW to:", "[CONSULT to:", "[LEAVE]",
        "[INVITE:", "[RELEASE:", "[CLOSE]", "[REQUEST_APPROVAL:", "[ADJUST_BUDGET:",
        "[MODEL:", "[FORK_CONSULTATION:", "[SWITCH_CANVAS:"];
    let mut content_lines = Vec::new();
    for &line in &lines[start..] {
        let trimmed = line.trim();
        if block_starters.iter().any(|s| trimmed.starts_with(s)) {
            break;
        }
        content_lines.push(line);
    }
    content_lines.join("\n")
}
