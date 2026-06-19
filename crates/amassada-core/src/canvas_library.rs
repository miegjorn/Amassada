use std::collections::HashMap;
use crate::canvas::Canvas;

const DEBATE: &str = include_str!("../../../canvases/stdlib/debate.yaml");
const DESIGN_SESSION: &str = include_str!("../../../canvases/stdlib/design-session.yaml");
const ARCHITECTURAL_DESIGN: &str = include_str!("../../../canvases/stdlib/architectural-design.yaml");
const CODE_REVIEW: &str = include_str!("../../../canvases/stdlib/code-review.yaml");
const CODE_REVIEW_COUNCIL: &str = include_str!("../../../canvases/stdlib/code-review-council.yaml");
const PLANNING: &str = include_str!("../../../canvases/stdlib/planning.yaml");
const IMPLEMENT_SESSION: &str = include_str!("../../../canvases/stdlib/implement-session.yaml");
const PROJECT_INTAKE: &str = include_str!("../../../canvases/stdlib/project-intake.yaml");
const GOVERNANCE_DELIBERATION: &str = include_str!("../../../canvases/stdlib/governance-deliberation.yaml");

/// Returns all stdlib canvases keyed by their `id` field.
/// Panics on malformed YAML — stdlib canvases are compile-time constants, parse failures are bugs.
pub fn stdlib() -> HashMap<String, Canvas> {
    [
        DEBATE,
        DESIGN_SESSION,
        ARCHITECTURAL_DESIGN,
        CODE_REVIEW,
        CODE_REVIEW_COUNCIL,
        PLANNING,
        IMPLEMENT_SESSION,
        PROJECT_INTAKE,
        GOVERNANCE_DELIBERATION,
    ]
    .iter()
    .map(|yaml| {
        let canvas = Canvas::from_yaml(yaml).expect("stdlib canvas YAML must parse");
        (canvas.id.clone(), canvas)
    })
    .collect()
}

/// Returns a closure suitable for passing to `MissionEngine::run()`.
pub fn stdlib_lookup() -> impl Fn(&str) -> Option<Canvas> {
    let map = stdlib();
    move |id: &str| map.get(id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdlib_loads_all_canvases() {
        let lib = stdlib();
        let expected = [
            "debate", "design-session", "architectural-design",
            "code-review", "code-review-council", "planning",
            "implement-session", "project-intake", "governance-deliberation",
        ];
        for id in expected {
            assert!(lib.contains_key(id), "missing canvas: {}", id);
        }
    }

    #[test]
    fn stdlib_lookup_returns_canvas_by_id() {
        let lookup = stdlib_lookup();
        assert!(lookup("debate").is_some());
        assert!(lookup("nonexistent").is_none());
    }
}
