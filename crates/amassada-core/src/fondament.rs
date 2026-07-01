use fondament_core::tree::DefinitionTree;
use fondament_core::types::{ComposedPart, PartKind};
use crate::error::{AmassadaError, Result};

pub struct ResolvedPersona {
    /// The `context` field of the Fondament definition — injected as domain context
    /// into the system prompt.
    pub context: String,
    /// Composition parts for the aporia preamble. Contains at minimum one
    /// Domain part keyed to the persona id; the extends chain contributes Discipline
    /// and Stance parts when present.
    pub collected_parts: Vec<ComposedPart>,
}

/// Resolve a Fondament persona by ID from a local Fondament checkout.
///
/// `fondament_path` is the root of the Fondament repo (the directory that contains
/// `definitions/`). `persona_id` is the logical Fondament id (e.g.
/// `"fondament/guilhem"`, `"fondament/projects/my-project"`).
///
/// Returns the persona's context string and the composition parts to use in the
/// aporia preamble. Returns `Err(Config)` when the definitions tree cannot
/// be loaded or the persona id is not found.
///
/// ## Resolution fallback chain (two-tier)
///
/// This function is **tier 1** — it queries the `DefinitionTree` (YAML-based structured
/// definitions under `definitions/`). If it succeeds, the `context` field and the full
/// `extends` chain (Discipline + Stance parts) are returned.
///
/// When this function returns `Err`, callers in `api.rs::post_message` fall back to
/// **tier 2**: `resolve_domain_context()`, which does a markdown scan across conventional
/// file layouts (`{domain}.md`, `{domain}/persona.md`, `{domain}/README.md`). If none of
/// those files exist either, a generic descriptor (`"You operate in the {domain} domain."`)
/// is produced as a last resort so dispatch never hard-fails on missing persona context.
///
/// The receiving agent pod (e.g. Guilhem) holds its own authoritative context regardless;
/// the fallback exists so Amassada can still assemble a minimal system prompt.
pub fn resolve_persona(fondament_path: &str, persona_id: &str) -> Result<ResolvedPersona> {
    let defs_path = std::path::Path::new(fondament_path).join("definitions");
    let tree = DefinitionTree::load(&defs_path)
        .map_err(|e| AmassadaError::Config(format!("fondament tree load failed: {}", e)))?;

    let def = tree.get(persona_id)
        .ok_or_else(|| AmassadaError::Config(
            format!("fondament definition '{}' not found in {}", persona_id, defs_path.display())
        ))?;

    let context = def.context.clone().unwrap_or_default();

    // Seed collected_parts with the top-level Domain part for this persona, then
    // walk the extends chain to collect Discipline and Stance parts.
    let mut collected_parts = vec![ComposedPart {
        kind: PartKind::Domain,
        name: persona_id.to_string(),
        weight: 0.0,
        corpus_ref: None,
    }];

    for parent_id in &def.extends {
        if let Some(parent) = tree.get(parent_id) {
            let part_kind = match parent.kind.as_str() {
                "discipline" => PartKind::Discipline,
                "stance"     => PartKind::Stance,
                _            => PartKind::Domain,
            };
            collected_parts.push(ComposedPart {
                kind: part_kind,
                name: parent_id.clone(),
                weight: 0.0,
                corpus_ref: None,
            });
        }
    }

    Ok(ResolvedPersona { context, collected_parts })
}
