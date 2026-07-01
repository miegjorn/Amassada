use amassada_core::canvas::{Canvas, CanvasLibrary};
use std::path::PathBuf;

#[test]
fn parses_debate_canvas() {
    let yaml = r#"
id: debate
version: "1.0.0"
mode: auto
selector:
  description: "Two agents debate"
  tags: [debate, argument]
  examples: ["should we use X or Y?"]
initial_participants:
  - persona: moderator
    domain: fondament/tech-moderator
  - persona: builder
    domain: fondament/senior-engineer
budget:
  total_tokens: 100000
  pools:
    main_session: 80000
    consultations: 15000
    mod_whisper: 5000
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 2
  max: 5
  convergence_modifier: 0.8
  context_window: 20
human:
  slot: false
output:
  format: markdown
  sections:
    - id: decision
      title: "Decision"
      required: true
"#;
    let canvas: Canvas = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(canvas.id, "debate");
    assert_eq!(canvas.budget.total_tokens, 100000);
    assert_eq!(canvas.initial_participants.len(), 2);
    assert!(canvas.initial_participants[0].is_moderator());
}

#[test]
fn canvas_selector_finds_best_match() {
    let library = CanvasLibrary::from_stdlib_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("canvases/stdlib")
    ).unwrap();
    let (canvas, score) = library.select("Should we use microservices or a monolith?");
    assert_eq!(canvas.id, "debate");
    assert!(score > 0.0);
}

#[test]
fn participant_endpoint_field_parses_and_reports() {
    let yaml = r#"
id: org-session
version: "1.0.0"
mode: interactive
selector:
  description: "Org conversation"
  tags: [org]
  examples: ["state of the stack?"]
initial_participants:
  - persona: guilhem
    domain: fondament/guilhem
    modifiers: [aporia]
    endpoint: "http://guilhem.agents.svc.cluster.local:8080"
  - persona: builder
    domain: fondament/senior-engineer
  # Example of complementary multi-model (Anthropic + Grok):
  # One participant uses grok-3 + endpoint for a reviewer/adversarial role.
  # Another can stay on claude (no endpoint => direct Anthropic dispatch).
  # Note: domain uses a real Fondament definition (code-reviewer) so that the
  # example is valid for both parsing tests and potential future runtime use.
  - persona: grok-adversary
    domain: fondament/code-reviewer
    model: "grok-3"
    endpoint: "http://grok-adversary.agents.svc.cluster.local:8080"
budget:
  total_tokens: 100000
  pools:
    main_session: 80000
    consultations: 15000
    mod_whisper: 5000
consultation:
  max_turns: 2
  min_response_tokens: 50
rounds:
  min: 1
  max: 999
  convergence_modifier: 1.0
  context_window: 20
human:
  slot: true
output:
  format: markdown
  sections: []
"#;
    let canvas = Canvas::from_yaml(yaml).expect("org-session canvas must parse");
    let guilhem = &canvas.initial_participants[0];
    assert!(guilhem.has_endpoint(), "guilhem participant must report an endpoint");
    assert_eq!(
        guilhem.endpoint.as_deref(),
        Some("http://guilhem.agents.svc.cluster.local:8080")
    );

    // A participant without an endpoint must default to None / has_endpoint() == false.
    let builder = &canvas.initial_participants[1];
    assert!(!builder.has_endpoint(), "participant without endpoint must report no endpoint");
    assert_eq!(builder.endpoint, None);

    // Mixed model participant (grok via endpoint)
    let grok_adv = &canvas.initial_participants[2];
    assert_eq!(grok_adv.model.as_deref(), Some("grok-3"));
    assert!(grok_adv.has_endpoint());
    assert_eq!(grok_adv.endpoint.as_deref(), Some("http://grok-adversary.agents.svc.cluster.local:8080"));
}
