use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct AgentsFile { agents: Vec<AgentDef> }

#[derive(Debug, Deserialize)]
struct AgentDef {
    id: String,
    name: String,
    mono: String,
    color: String,
    detect: DetectDef,
    state: StateDef,
    chrome: Vec<String>,
    reply_prefix: Vec<String>,
    input_prefix: Vec<String>,
    #[serde(default)] input_zone_after: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DetectDef {
    #[serde(default)] osc_title_contains: Vec<String>,
    #[serde(default)] screen_contains: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StateDef { thinking: StateMatch, idle: StateMatch }

#[derive(Debug, Deserialize)]
struct StateMatch {
    #[serde(default)] osc_title_prefix: Vec<String>,
    #[serde(default)] screen_regex: Vec<String>,
}

/// Compiled agent config ready for matching
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub mono: String,
    pub color: String,
    detect_osc: Vec<String>,
    detect_screen: Vec<String>,
    thinking_osc_prefix: Vec<String>,
    thinking_screen: Vec<Regex>,
    idle_osc_prefix: Vec<String>,
    idle_screen: Vec<Regex>,
    chrome_patterns: Vec<Regex>,
    pub reply_prefix: Vec<String>,
    pub input_prefix: Vec<String>,
    input_zone_after: Option<Regex>,
}

static AGENTS: OnceLock<Vec<AgentConfig>> = OnceLock::new();

pub fn agents() -> &'static Vec<AgentConfig> {
    AGENTS.get_or_init(|| {
        let json = include_str!("../agents.json");
        let file: AgentsFile = serde_json::from_str(json).expect("Failed to parse agents.json");
        file.agents.into_iter().map(|a| {
            AgentConfig {
                id: a.id, name: a.name, mono: a.mono, color: a.color,
                detect_osc: a.detect.osc_title_contains,
                detect_screen: a.detect.screen_contains,
                thinking_osc_prefix: a.state.thinking.osc_title_prefix,
                thinking_screen: compile_regexes(&a.state.thinking.screen_regex),
                idle_osc_prefix: a.state.idle.osc_title_prefix,
                idle_screen: compile_regexes(&a.state.idle.screen_regex),
                chrome_patterns: compile_regexes(&a.chrome),
                reply_prefix: a.reply_prefix,
                input_prefix: a.input_prefix,
                input_zone_after: a.input_zone_after.as_deref().and_then(|p| Regex::new(&format!("(?i){}", p)).ok()),
            }
        }).collect()
    })
}

fn compile_regexes(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(&format!("(?i){}", p)).ok()).collect()
}

impl AgentConfig {
    pub fn detect_from_title(&self, title: &str) -> bool {
        let t = title.to_lowercase();
        self.detect_osc.iter().any(|s| t.contains(&s.to_lowercase()))
    }

    pub fn detect_from_content(&self, content: &str) -> bool {
        self.detect_screen.iter().any(|s| content.contains(s))
    }

    pub fn detect_state_from_title(&self, title: &str) -> Option<&'static str> {
        for p in &self.thinking_osc_prefix {
            if title.starts_with(p) { return Some("thinking"); }
        }
        for p in &self.idle_osc_prefix {
            if title.starts_with(p) { return Some("idle"); }
        }
        None
    }

    pub fn detect_state_from_screen(&self, rows: &[String]) -> Option<&'static str> {
        // Thinking takes priority: scan ALL rows for thinking indicators first
        for row in rows.iter() {
            let t = row.trim();
            if t.is_empty() { continue; }
            for re in &self.thinking_screen {
                if re.is_match(t) { return Some("thinking"); }
            }
        }
        // No thinking found: check for idle (bottom-up)
        for row in rows.iter().rev() {
            let t = row.trim();
            if t.is_empty() { continue; }
            for re in &self.idle_screen {
                if re.is_match(t) { return Some("idle"); }
            }
        }
        None
    }

    pub fn is_chrome(&self, line: &str) -> bool {
        let t = line.trim();
        // Check regex patterns
        for re in &self.chrome_patterns {
            if re.is_match(t) { return true; }
        }
        // Check input prefixes
        for p in &self.input_prefix {
            if t.starts_with(p) { return true; }
        }
        false
    }

    pub fn strip_reply_prefix<'a>(&self, line: &'a str) -> &'a str {
        let t = line.trim();
        for p in &self.reply_prefix {
            if t.starts_with(p) { return t[p.len()..].trim(); }
        }
        t
    }

    /// Returns true if this line marks the start of the input zone (everything below is user input)
    pub fn is_input_zone_boundary(&self, line: &str) -> bool {
        if let Some(ref re) = self.input_zone_after {
            re.is_match(line.trim())
        } else {
            false
        }
    }

    pub fn has_input_zone(&self) -> bool {
        self.input_zone_after.is_some()
    }
}

/// Find which agent matches the given title or content
pub fn detect_agent(title: Option<&str>, content: &str) -> Option<&'static AgentConfig> {
    let agents = agents();
    if let Some(t) = title {
        for a in agents { if a.detect_from_title(t) { return Some(a); } }
    }
    // Kiro first (before Claude, since Kiro output might mention "claude")
    for a in agents {
        if a.id == "kiro" && a.detect_from_content(content) { return Some(a); }
    }
    for a in agents {
        if a.id != "kiro" && a.detect_from_content(content) { return Some(a); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_claude_thinking_detection() {
        let agents = agents();
        let claude = agents.iter().find(|a| a.id == "claude").unwrap();
        assert_eq!(claude.detect_state_from_screen(&["✶ Crafting…".to_string()]), Some("thinking"));
        assert_eq!(claude.detect_state_from_screen(&["✢ Proofing…".to_string()]), Some("thinking"));
        assert_eq!(claude.detect_state_from_screen(&["❯".to_string()]), Some("idle"));
    }
    #[test]
    fn test_kiro_thinking_detection() {
        let agents = agents();
        let kiro = agents.iter().find(|a| a.id == "kiro").unwrap();
        assert_eq!(kiro.detect_state_from_screen(&["⠀ Thinking... (esc to cancel)".to_string()]), Some("thinking"));
        assert_eq!(kiro.detect_state_from_screen(&["Kiro is working · type to queue".to_string()]), Some("thinking"));
        assert_eq!(kiro.detect_state_from_screen(&["Kiro · auto · ◔ 5%".to_string()]), Some("idle"));
    }
    #[test]
    fn test_codex_thinking_detection() {
        let agents = agents();
        let codex = agents.iter().find(|a| a.id == "codex").unwrap();
        assert_eq!(codex.detect_state_from_screen(&["• Working (0s • esc to interrupt)".to_string()]), Some("thinking"));
        assert_eq!(codex.detect_state_from_screen(&["gpt-5.4 xhigh · ~/project".to_string()]), Some("idle"));
    }
}
