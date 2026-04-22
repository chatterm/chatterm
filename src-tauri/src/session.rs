use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub kind: String,            // "shell" | "agent"
    pub agent: Option<String>,   // "claude" | "kiro" | "codex" | etc
    pub command: Option<String>, // original launch command
    pub cwd: Option<String>,
    pub pinned: bool,
}

fn meta_path() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into())
        });
        PathBuf::from(base).join("chatterm").join("sessions.json")
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".chatterm").join("sessions.json")
    }
}

pub fn load() -> Vec<SessionMeta> {
    let path = meta_path();
    if !path.exists() {
        return vec![];
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(sessions: &[SessionMeta]) {
    let path = meta_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    if let Ok(json) = serde_json::to_string_pretty(sessions) {
        std::fs::write(&path, json).ok();
    }
}
