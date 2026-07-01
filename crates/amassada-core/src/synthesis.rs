use futures::future::join_all;
use crate::canvas::OutputSection;
use crate::context::ContextBuilder;
use crate::dispatch::{self, TurnRequest};
use crate::error::Result;
use crate::types::OutputArtifact;

const SYNTHESIS_MODEL: &str = "claude-sonnet-4-6";

pub async fn synthesize_artifacts(
    sections: &[OutputSection],
    context_builder: &ContextBuilder,
    canvas_id: &str,
    goal: &str,
) -> Result<Vec<OutputArtifact>> {
    let context = context_builder.build_for(
        &crate::types::AgentId::new("synthesis"),
        vec![],
        None,
    );

    let _ = canvas_id; // reserved for future canvas-specific synthesis instructions

    let futures: Vec<_> = sections.iter().map(|section| {
        let ctx = context.clone();
        let section = section.clone();
        let goal = goal.to_string();
        async move {
            let system = format!(
                "You are a synthesis agent. Based on the session transcript, produce the '{}' section of the session output. Be concise and precise. The session goal was: {}",
                section.title, goal
            );
            let req = TurnRequest {
                system_prompt: system,
                context: ctx,
                model: SYNTHESIS_MODEL.to_string(),
                max_tokens: 2048,
                structured_reasoning: None,
                api_key: None,
                shared_context: None,
        mcp_scopes: vec![],
            };
            let response = dispatch::dispatch(req).await?;
            Ok::<OutputArtifact, crate::error::AmassadaError>(OutputArtifact {
                id: section.id,
                title: section.title,
                content: response.text,
                required: section.required,
            })
        }
    }).collect();

    let results: Vec<Result<OutputArtifact>> = join_all(futures).await;
    results.into_iter().collect()
}
