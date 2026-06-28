use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: String,
    pub version: String,
    pub mode: CanvasMode,
    pub selector: SelectorMeta,
    pub initial_participants: Vec<ParticipantDef>,
    pub budget: BudgetConfig,
    pub consultation: ConsultationConfig,
    pub rounds: RoundsConfig,
    pub human: HumanConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CanvasMode { Auto, Interactive }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectorMeta {
    pub description: String,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantDef {
    pub persona: String,
    #[serde(default)]
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
}

impl ParticipantDef {
    pub fn is_moderator(&self) -> bool { self.persona == "moderator" }
    pub fn is_human(&self) -> bool { self.persona == "human" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub total_tokens: u32,
    pub pools: BudgetPools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetPools {
    pub main_session: u32,
    pub consultations: u32,
    pub mod_whisper: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsultationConfig {
    pub max_turns: u32,
    pub min_response_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundsConfig {
    pub min: u32,
    pub max: u32,
    pub convergence_modifier: f32,
    pub context_window: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanConfig {
    pub slot: bool,
    #[serde(default = "default_advisory_window")]
    pub advisory_window_turns: u32,
}

fn default_advisory_window() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub sections: Vec<OutputSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSection {
    pub id: String,
    pub title: String,
    pub required: bool,
}

impl Canvas {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| AmassadaError::CanvasParse(e.to_string()))
    }
}

pub struct CanvasLibrary {
    pub(crate) canvases: Vec<Canvas>,
}

impl CanvasLibrary {
    pub fn from_stdlib_dir(dir: PathBuf) -> Result<Self> {
        let mut canvases = Vec::new();
        if !dir.exists() { return Ok(Self { canvases }); }
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| AmassadaError::CanvasParse(e.to_string()))?
        {
            let path = entry.map_err(|e| AmassadaError::CanvasParse(e.to_string()))?.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                let yaml = std::fs::read_to_string(&path)
                    .map_err(|e| AmassadaError::CanvasParse(e.to_string()))?;
                match Canvas::from_yaml(&yaml) {
                    Ok(c) => canvases.push(c),
                    Err(e) => tracing::warn!("Skipping canvas {:?}: {}", path, e),
                }
            }
        }
        Ok(Self { canvases })
    }

    pub fn get(&self, id: &str) -> Option<&Canvas> {
        self.canvases.iter().find(|c| c.id == id)
    }

    /// Heuristic selector: score by keyword overlap between query and canvas metadata.
    /// Returns the best-match canvas and a confidence score in [0, 1].
    /// Use `select_canvas_with_llm` for a smarter selection.
    pub fn select(&self, query: &str) -> (&Canvas, f32) {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut best_score = -1.0f32;
        let mut best_idx = 0;

        for (i, canvas) in self.canvases.iter().enumerate() {
            let haystack = format!(
                "{} {} {}",
                canvas.selector.description,
                canvas.selector.tags.join(" "),
                canvas.selector.examples.join(" ")
            ).to_lowercase();

            let matches = query_words.iter().filter(|w| haystack.contains(*w)).count();
            let score = if query_words.is_empty() { 0.0 } else {
                matches as f32 / query_words.len() as f32
            };

            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        (&self.canvases[best_idx], best_score.max(0.0))
    }
}

/// Asks Haiku to pick the best canvas for `query` from the provided library.
/// Returns the canvas id of the best match, falling back to sync heuristic on error.
pub async fn select_canvas_with_llm(query: &str, library: &CanvasLibrary) -> String {
    use crate::dispatch::{dispatch, TurnRequest};

    let menu: Vec<String> = library.canvases.iter()
        .map(|c| format!("- id: {}\n  description: {}\n  tags: {}",
            c.id, c.selector.description, c.selector.tags.join(", ")))
        .collect();
    let menu_text = menu.join("\n");

    let system = "You are a canvas selector. Given a user request and a list of available canvases, \
        respond with ONLY the id of the best-matching canvas. No explanation, no punctuation — just the id.";
    let context = format!("REQUEST:\n{}\n\nAVAILABLE CANVASES:\n{}", query, menu_text);

    match dispatch(TurnRequest {
        system_prompt: system.into(),
        context,
        model: "claude-haiku-4-5-20251001".into(),
        max_tokens: 32,
        thinking_budget: None,
        api_key: None,
        shared_context: None,
    }).await {
        Ok(resp) => {
            let id = resp.text.trim().to_string();
            if library.get(&id).is_some() { id }
            else { library.select(query).0.id.clone() } // fallback
        }
        Err(_) => library.select(query).0.id.clone(), // fallback
    }
}
