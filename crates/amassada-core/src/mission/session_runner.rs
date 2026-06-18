use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use uuid::Uuid;
use crate::canvas::Canvas;
use crate::error::Result;
use crate::session::SessionEngine;
use crate::transport::Transport;
use crate::types::SessionOutput;

/// Abstraction over SessionEngine for testability.
/// MissionEngine calls runner.run(canvas, goal) instead of constructing SessionEngine directly.
#[async_trait]
pub trait SessionRunner: Send + Sync {
    async fn run(&self, canvas: Canvas, goal: String) -> Result<SessionOutput>;
}

/// Scales canvas budget pools proportionally to a new total.
/// Called by MissionEngine before passing canvas to runner.
pub fn scale_canvas_budget(mut canvas: Canvas, budget_tokens: u64) -> Canvas {
    let orig = canvas.budget.total_tokens as u64;
    if orig == 0 || budget_tokens == orig {
        return canvas;
    }
    let scale = budget_tokens as f64 / orig as f64;
    canvas.budget.total_tokens = budget_tokens.min(u32::MAX as u64) as u32;
    canvas.budget.pools.main_session =
        ((canvas.budget.pools.main_session as f64 * scale) as u32).max(1);
    canvas.budget.pools.consultations =
        ((canvas.budget.pools.consultations as f64 * scale) as u32).max(1);
    canvas.budget.pools.mod_whisper =
        ((canvas.budget.pools.mod_whisper as f64 * scale) as u32).max(1);
    canvas
}

/// Production runner — wraps a real SessionEngine with a shared transport.
pub struct DefaultSessionRunner {
    transport: Arc<dyn Transport>,
}

impl DefaultSessionRunner {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl SessionRunner for DefaultSessionRunner {
    async fn run(&self, canvas: Canvas, goal: String) -> Result<SessionOutput> {
        SessionEngine::new(canvas, goal, Arc::clone(&self.transport))
            .run()
            .await
    }
}

/// Test double — returns pre-baked SessionOutput values from a queue.
pub struct MockSessionRunner {
    outputs: Mutex<Vec<SessionOutput>>,
}

impl MockSessionRunner {
    pub fn new(outputs: Vec<SessionOutput>) -> Self {
        Self { outputs: Mutex::new(outputs) }
    }
}

#[async_trait]
impl SessionRunner for MockSessionRunner {
    async fn run(&self, _canvas: Canvas, goal: String) -> Result<SessionOutput> {
        let mut q = self.outputs.lock().unwrap();
        Ok(if q.is_empty() {
            SessionOutput {
                session_id: Uuid::new_v4().to_string(),
                canvas_id: "mock".into(),
                goal,
                artifacts: vec![],
                total_tokens: 0,
            }
        } else {
            q.remove(0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OutputArtifact;

    const STUB_CANVAS_YAML: &str = "id: debate\nversion: \"1\"\nmode: auto\nselector:\n  description: d\n  tags: []\n  examples: []\ninitial_participants: []\nbudget:\n  total_tokens: 10000\n  pools:\n    main_session: 8000\n    consultations: 1500\n    mod_whisper: 500\nconsultation:\n  max_turns: 3\n  min_response_tokens: 50\nrounds:\n  min: 1\n  max: 5\n  convergence_modifier: 0.8\n  context_window: 8192\nhuman:\n  slot: false\noutput:\n  format: markdown\n  sections: []";

    fn make_output(content: &str) -> SessionOutput {
        SessionOutput {
            session_id: "s1".into(),
            canvas_id: "debate".into(),
            goal: "test goal".into(),
            artifacts: vec![OutputArtifact {
                id: "a1".into(),
                title: "Result".into(),
                content: content.into(),
                required: true,
            }],
            total_tokens: 5_000,
        }
    }

    #[tokio::test]
    async fn mock_runner_returns_queued_outputs() {
        let runner = MockSessionRunner::new(vec![
            make_output("first artifact"),
            make_output("second artifact"),
        ]);
        let canvas = Canvas::from_yaml(STUB_CANVAS_YAML).unwrap();
        let out1 = runner.run(canvas.clone(), "goal 1".into()).await.unwrap();
        assert_eq!(out1.artifacts[0].content, "first artifact");
        let out2 = runner.run(canvas, "goal 2".into()).await.unwrap();
        assert_eq!(out2.artifacts[0].content, "second artifact");
    }

    #[tokio::test]
    async fn scale_canvas_budget_scales_proportionally() {
        let canvas = Canvas::from_yaml(STUB_CANVAS_YAML).unwrap();
        // Original: total=10000, main=8000, consult=1500, whisper=500
        let scaled = scale_canvas_budget(canvas, 5_000);
        assert_eq!(scaled.budget.total_tokens, 5_000);
        // Pools should be roughly half
        assert!(scaled.budget.pools.main_session > 0);
        assert!(scaled.budget.pools.consultations > 0);
        assert!(scaled.budget.pools.mod_whisper > 0);
    }
}
