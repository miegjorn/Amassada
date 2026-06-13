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
