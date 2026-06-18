use crate::mission::types::{
    MissionBudget, SessionRecord, SubObjective, SubObjectiveStatus,
};

/// Snapshot of MissionEngine state used to build the mission envelope.
/// Uses borrowed references to avoid cloning the engine state.
pub struct EnvelopeState<'a> {
    pub goal: &'a str,
    pub completion_condition: &'a str,
    pub sub_objectives: &'a [SubObjective],
    pub budget: &'a MissionBudget,
    pub sessions_run: &'a [SessionRecord],
}

pub fn build_mission_envelope(state: &EnvelopeState<'_>) -> String {
    let mut buf = String::new();

    buf.push_str(&format!("MISSION GOAL\n{}\n\n", state.goal));
    buf.push_str(&format!("COMPLETION CONDITION\n{}\n\n", state.completion_condition));

    buf.push_str("SUB-OBJECTIVES\n");
    for obj in state.sub_objectives {
        let marker = match obj.status {
            SubObjectiveStatus::Complete => "✓",
            SubObjectiveStatus::InProgress => "→",
            SubObjectiveStatus::OutOfScope => "✗",
            SubObjectiveStatus::Pending => " ",
        };
        buf.push_str(&format!("  [{}] {}: {}\n", marker, obj.id, obj.description));
        if let Some(out) = &obj.output {
            buf.push_str(&format!("       output: {}\n", truncate(out, 200)));
        }
        if let Some(reason) = &obj.last_eval_reason {
            buf.push_str(&format!("       last eval: \"{}\"\n", reason));
        }
    }

    buf.push('\n');
    buf.push_str("BUDGET\n");
    buf.push_str(&format!(
        "  deployable: {} / {} tokens used\n",
        state.budget.deployable_spent, state.budget.deployable
    ));
    buf.push_str(&format!(
        "  discretionary strategize: {} / {} tokens used\n",
        state.budget.discretionary_strategize_spent, state.budget.discretionary
    ));
    buf.push_str(&format!(
        "  discretionary evaluate: {} / {} tokens used\n",
        state.budget.discretionary_evaluate_spent, state.budget.discretionary
    ));
    buf.push_str(&format!("  sessions run: {}\n", state.sessions_run.len()));

    if !state.sessions_run.is_empty() {
        buf.push_str("\nSESSIONS RUN\n");
        for record in state.sessions_run {
            let status = match &record.evaluation {
                Some(e) if e.satisfied => "satisfied".to_string(),
                Some(e) => format!("not satisfied — {}", e.reason),
                None => "pending evaluation".to_string(),
            };
            buf.push_str(&format!(
                "  {} ({}): [{}] — {}\n",
                record.session_id,
                record.canvas_id,
                record.sub_objective_ids.join(", "),
                status,
            ));
        }
    }

    buf
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::types::{
        EvaluationResult, MissionBudget, SessionRecord, SubObjective, SubObjectiveStatus,
    };

    #[test]
    fn envelope_contains_all_sections() {
        let sub_objectives = vec![
            SubObjective {
                id: "obj-1".into(),
                description: "surface trade-offs".into(),
                completion_condition: "pros/cons listed for at least two options".into(),
                status: SubObjectiveStatus::Complete,
                output: Some("JWT vs sessions comparison...".into()),
                last_eval_reason: None,
            },
            SubObjective {
                id: "obj-2".into(),
                description: "pick one approach".into(),
                completion_condition: "one option chosen with rationale".into(),
                status: SubObjectiveStatus::Pending,
                output: None,
                last_eval_reason: Some("no decision made yet".into()),
            },
        ];

        let mut budget = MissionBudget::new(100_000);
        budget.deployable_spent = 10_000;
        budget.discretionary_strategize_spent = 2_000;

        let sessions_run = vec![SessionRecord {
            session_id: "s1".into(),
            canvas_id: "debate".into(),
            budget_allocated: 10_000,
            budget_spent: 9_800,
            sub_objective_ids: vec!["obj-1".into()],
            artifact: Some("JWT vs sessions comparison...".into()),
            evaluation: Some(EvaluationResult {
                satisfied: true,
                reason: "pros/cons listed".into(),
            }),
        }];

        let state = EnvelopeState {
            goal: "decide on auth approach",
            completion_condition: "a decision doc exists naming one chosen approach",
            sub_objectives: &sub_objectives,
            budget: &budget,
            sessions_run: &sessions_run,
        };

        let env = build_mission_envelope(&state);

        assert!(env.contains("MISSION GOAL"), "missing MISSION GOAL section");
        assert!(env.contains("decide on auth approach"), "missing goal text");
        assert!(
            env.contains("COMPLETION CONDITION"),
            "missing COMPLETION CONDITION section"
        );
        assert!(
            env.contains("a decision doc exists"),
            "missing condition text"
        );
        assert!(env.contains("SUB-OBJECTIVES"), "missing SUB-OBJECTIVES section");
        assert!(env.contains("[✓] obj-1"), "missing completed obj marker");
        assert!(env.contains("[ ] obj-2"), "missing pending obj marker");
        assert!(
            env.contains("last eval: \"no decision made yet\""),
            "missing last eval reason"
        );
        assert!(env.contains("BUDGET"), "missing BUDGET section");
        assert!(env.contains("10000 / 80000"), "missing deployable spend display");
        assert!(env.contains("SESSIONS RUN"), "missing SESSIONS RUN section");
        assert!(env.contains("s1 (debate)"), "missing session record");
        assert!(env.contains("satisfied"), "missing evaluation status");
    }
}
