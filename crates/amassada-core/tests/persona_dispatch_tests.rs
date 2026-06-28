use amassada_core::fondament::{resolve_persona, ResolvedPersona};
use fondament_core::types::PartKind;
use std::io::Write;
use tempfile::TempDir;

/// Write a minimal Fondament definitions layout into a tempdir and return the
/// path that acts as the fondament_path root (i.e. the dir that contains "definitions/").
fn make_fondament_root(defs: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (id, context) in defs {
        // id like "fondament/projects/alpha" → file at definitions/fondament/projects/alpha.yaml
        let rel = id.replace('/', std::path::MAIN_SEPARATOR_STR);
        let file_path = dir.path().join("definitions").join(format!("{}.yaml", rel));
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = format!(
            "id: {id}\nkind: project-agent\ndefault_model: claude-sonnet-4-6\ncontext: |\n  {context}\n"
        );
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
    }
    dir
}

#[test]
fn resolve_persona_returns_correct_context() {
    let root = make_fondament_root(&[
        ("fondament/projects/alpha", "You are the Alpha project agent."),
    ]);
    let persona = resolve_persona(
        root.path().to_str().unwrap(),
        "fondament/projects/alpha",
    ).expect("should resolve alpha");

    assert!(
        persona.context.contains("Alpha project agent"),
        "context should contain the persona text, got: {:?}", persona.context
    );
}

#[test]
fn two_projects_yield_distinct_contexts() {
    let root = make_fondament_root(&[
        ("fondament/projects/alpha", "You are the Alpha project agent."),
        ("fondament/projects/beta",  "You are the Beta project agent."),
    ]);
    let fondament_path = root.path().to_str().unwrap();

    // Resolve both — simulates two concurrent sessions dispatching in parallel.
    let (alpha, beta) = (
        resolve_persona(fondament_path, "fondament/projects/alpha").unwrap(),
        resolve_persona(fondament_path, "fondament/projects/beta").unwrap(),
    );

    assert_ne!(alpha.context, beta.context, "personas must produce distinct contexts");
    assert!(alpha.context.contains("Alpha"), "alpha preamble must mention Alpha");
    assert!(beta.context.contains("Beta"),   "beta preamble must mention Beta");
}

#[test]
fn resolve_persona_sets_domain_part() {
    let root = make_fondament_root(&[
        ("fondament/projects/gamma", "Gamma context."),
    ]);
    let persona = resolve_persona(
        root.path().to_str().unwrap(),
        "fondament/projects/gamma",
    ).unwrap();

    assert_eq!(persona.collected_parts.len(), 1);
    assert!(matches!(persona.collected_parts[0].kind, PartKind::Domain));
    assert_eq!(persona.collected_parts[0].name, "fondament/projects/gamma");
}

#[test]
fn unknown_persona_id_returns_error() {
    let root = make_fondament_root(&[]);
    let err = resolve_persona(
        root.path().to_str().unwrap(),
        "fondament/projects/nonexistent",
    );
    assert!(err.is_err(), "unknown persona should return an error");
}

#[test]
fn guilhem_entry_resolves_correctly() {
    // Guilhem continues to work — its entry in the registry points to fondament/guilhem.
    let root = make_fondament_root(&[
        ("fondament/guilhem", "You are Guilhem de Tudela, chronicler of the Occitan stack."),
    ]);
    let persona = resolve_persona(
        root.path().to_str().unwrap(),
        "fondament/guilhem",
    ).unwrap();

    assert!(persona.context.contains("Guilhem"), "guilhem persona must resolve");
}
