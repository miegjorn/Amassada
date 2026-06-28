use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::{AmassadaError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub fondament_persona: String,
    pub matrix_rooms: Vec<String>,
    pub farga_project: String,
}

/// Maps project ids and Matrix room ids to their `ProjectEntry`.
/// Loaded once at startup from a TOML file; add a project by editing the file and restarting.
#[derive(Debug, Clone, Default)]
pub struct ProjectRegistry {
    by_id: HashMap<String, ProjectEntry>,
    /// room_id → project id, for reverse lookup from an incoming Matrix event.
    by_room: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RegistryFile {
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

impl ProjectRegistry {
    pub fn from_toml(content: &str) -> Result<Self> {
        let file: RegistryFile = if content.trim().is_empty() {
            RegistryFile { projects: vec![] }
        } else {
            toml::from_str(content)
                .map_err(|e| AmassadaError::Config(format!("projects.toml parse error: {}", e)))?
        };

        let mut by_id = HashMap::new();
        let mut by_room = HashMap::new();
        for entry in file.projects {
            for room in &entry.matrix_rooms {
                by_room.insert(room.clone(), entry.id.clone());
            }
            by_id.insert(entry.id.clone(), entry);
        }
        Ok(Self { by_id, by_room })
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AmassadaError::Config(format!("cannot read {}: {}", path.display(), e)))?;
        Self::from_toml(&content)
    }

    pub fn get_by_id(&self, id: &str) -> Option<&ProjectEntry> {
        self.by_id.get(id)
    }

    pub fn get_by_room(&self, room_id: &str) -> Option<&ProjectEntry> {
        self.by_room.get(room_id).and_then(|id| self.by_id.get(id))
    }
}
