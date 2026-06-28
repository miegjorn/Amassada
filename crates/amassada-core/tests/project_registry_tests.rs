use amassada_core::project_registry::ProjectRegistry;

const TWO_PROJECTS_TOML: &str = r#"
[[projects]]
id = "alpha"
fondament_persona = "fondament/projects/alpha.yaml"
matrix_rooms = ["!room1:occitane.guilhem", "!room2:occitane.guilhem"]
farga_project = "alpha"

[[projects]]
id = "beta"
fondament_persona = "fondament/projects/beta.yaml"
matrix_rooms = ["!room3:occitane.guilhem"]
farga_project = "beta"
"#;

#[test]
fn lookup_by_project_id() {
    let reg = ProjectRegistry::from_toml(TWO_PROJECTS_TOML).unwrap();

    let alpha = reg.get_by_id("alpha").expect("alpha should be found");
    assert_eq!(alpha.fondament_persona, "fondament/projects/alpha.yaml");
    assert_eq!(alpha.farga_project, "alpha");
    assert_eq!(alpha.matrix_rooms.len(), 2);

    let beta = reg.get_by_id("beta").expect("beta should be found");
    assert_eq!(beta.fondament_persona, "fondament/projects/beta.yaml");
    assert_eq!(beta.matrix_rooms, vec!["!room3:occitane.guilhem"]);
}

#[test]
fn lookup_by_room_id() {
    let reg = ProjectRegistry::from_toml(TWO_PROJECTS_TOML).unwrap();

    let via_room1 = reg.get_by_room("!room1:occitane.guilhem").expect("room1 → alpha");
    assert_eq!(via_room1.id, "alpha");

    let via_room3 = reg.get_by_room("!room3:occitane.guilhem").expect("room3 → beta");
    assert_eq!(via_room3.id, "beta");
}

#[test]
fn unknown_project_id_returns_none() {
    let reg = ProjectRegistry::from_toml(TWO_PROJECTS_TOML).unwrap();
    assert!(reg.get_by_id("nonexistent").is_none());
}

#[test]
fn unknown_room_id_returns_none() {
    let reg = ProjectRegistry::from_toml(TWO_PROJECTS_TOML).unwrap();
    assert!(reg.get_by_room("!unknown:occitane.guilhem").is_none());
}

#[test]
fn empty_toml_yields_empty_registry() {
    let reg = ProjectRegistry::from_toml("").unwrap();
    assert!(reg.get_by_id("anything").is_none());
}

#[test]
fn load_from_file_roundtrips() {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(TWO_PROJECTS_TOML.as_bytes()).unwrap();
    let reg = ProjectRegistry::load(tmp.path()).unwrap();
    assert!(reg.get_by_id("alpha").is_some());
    assert!(reg.get_by_id("beta").is_some());
}
