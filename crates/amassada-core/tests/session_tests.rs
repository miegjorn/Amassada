use std::sync::Arc;
use amassada_core::budget::BudgetLedger;
use amassada_core::canvas::Canvas;
use amassada_core::channels::whisper::WhisperQueue;
use amassada_core::context::ContextBuilder;
use amassada_core::graph::SessionGraph;
use amassada_core::round::RoundRunner;
use amassada_core::session::SessionEngine;
use amassada_core::transport::local::LocalTransport;
use amassada_core::types::ActiveParticipant;

/// Canvas with 2 rounds, no participants, and no output sections so the
/// session completes without any live API calls.
const STUB_CANVAS_YAML: &str = "\
id: test_canvas
version: \"1\"
mode: auto
selector:
  description: test
  tags: []
  examples: []
initial_participants: []
budget:
  total_tokens: 10000
  pools:
    main_session: 8000
    consultations: 1500
    mod_whisper: 500
consultation:
  max_turns: 3
  min_response_tokens: 50
rounds:
  min: 1
  max: 2
  convergence_modifier: 0.8
  context_window: 8192
human:
  slot: false
output:
  format: markdown
  sections: []
";

/// After 2 rounds each call `apply_delta` on the proposals delta (even when
/// empty), bumping `graph.version` by 1 per round.  With no ANTHROPIC_API_KEY
/// set the Haiku extraction is skipped non-fatally, so version == 2 (one
/// proposals-delta per round).
#[tokio::test]
async fn session_graph_version_increments_each_round() {
    let canvas = Canvas::from_yaml(STUB_CANVAS_YAML).unwrap();
    let transport = Arc::new(LocalTransport::new_test());
    let mut engine = SessionEngine::new(canvas, "test goal".to_string(), transport);

    engine.run().await.unwrap();

    // 2 rounds × 1 apply_delta (proposals) per round = version 2 minimum.
    // Extraction may also fire if ANTHROPIC_API_KEY is set in the env, giving a
    // higher version — so we assert >= 2 (deterministic lower bound).
    assert!(
        engine.graph.version >= 2,
        "graph.version should be >= 2 after 2 rounds (one proposals apply_delta per round); got {}",
        engine.graph.version
    );
}

/// Verify that `RoundRunner::run()` accepts and forwards `shared_context: Some(...)`
/// without error.  With zero participants the participant loop is a no-op so no
/// live dispatch call is made — the invariant that `shared_context` is wired into
/// every `TurnRequest` is tested at the `build_request_body` level in
/// `dispatch_tests.rs::dispatch_with_shared_context_uses_two_system_blocks`.
///
/// This test focuses on the `RoundRunner` API surface: calling `run(Some(...))`
/// on round 2 returns `Ok` and produces an empty, non-closing result.
#[tokio::test]
async fn round_runner_threads_shared_context() {
    let transport = LocalTransport::new_test();
    let mut participants: Vec<ActiveParticipant> = vec![];
    let mut context_builder = ContextBuilder::new(8192);
    let mut whisper_queue = WhisperQueue::new();
    let mut budget = BudgetLedger::new(10_000, 8_000, 1_500, 500);
    let graph = SessionGraph::new("test-session");

    let mut runner = RoundRunner {
        round_num: 2,
        participants: &mut participants,
        context_builder: &mut context_builder,
        whisper_queue: &mut whisper_queue,
        budget: &mut budget,
        transport: &transport,
        graph: &graph,
    };

    let result = runner
        .run(Some("shared_graph_context".to_string()))
        .await;

    let result = result.expect("RoundRunner::run with shared_context=Some should succeed");
    assert!(!result.should_close, "empty-participant round should not set should_close");
    assert!(result.agent_proposal_ops.is_empty() && result.moderator_proposal_ops.is_empty(), "no proposals expected with zero participants");
}
