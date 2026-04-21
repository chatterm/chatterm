use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::agent_config;
use crate::vscreen::VScreen;

#[derive(Debug, Clone, Serialize)]
pub struct PtyOutput {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PtyMeta {
    pub session_id: String,
    pub title: Option<String>,
    pub agent: Option<String>,
    pub state: Option<String>,
    pub preview: Option<String>,
    pub notification: Option<PtyNotification>,
    pub command: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PtyNotification {
    pub title: String,
    pub body: String,
    pub subtitle: Option<String>,
}

pub struct PtySession {
    master_write: Arc<Mutex<Box<dyn Write + Send>>>,
    master_pty: Option<Box<dyn MasterPty + Send>>,
    #[allow(dead_code)]
    pub id: String,
    _child: Option<Box<dyn portable_pty::Child + Send>>,
    _process: Option<std::process::Child>,
}

pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

pub struct CreateSessionRequest<'a> {
    pub id: &'a str,
    pub cols: u16,
    pub rows: u16,
    pub command: Option<&'a str>,
    pub cwd: Option<&'a str>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_session(
        &self,
        req: CreateSessionRequest<'_>,
        on_output: impl Fn(PtyOutput) + Send + 'static,
        on_meta: impl Fn(PtyMeta) + Send + 'static,
    ) -> Result<(), String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: req.rows,
                cols: req.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {e}"))?;

        let shell = detect_shell();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        if let Some(c) = req.command {
            cmd.args(["-c", c]);
        }

        // Set HOME and common env
        let home = home_dir();
        cmd.env("HOME", &home);
        let work_dir = req.cwd.unwrap_or(&home);
        cmd.cwd(work_dir);
        cmd.env("USER", whoami());
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", expand_path(&home, &path));
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("CHATTERM_SESSION_ID", req.id);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn: {e}"))?;

        // Reader thread
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone reader: {e}"))?;
        let session_id = req.id.to_string();
        thread::spawn(move || {
            let mut buf_reader = BufReader::new(reader);
            let mut buf = vec![0u8; 4096];
            let mut utf8_carry = Vec::new(); // leftover bytes from incomplete UTF-8 sequence
            let mut last_agent_cfg: Option<&agent_config::AgentConfig> = None;
            let mut last_state: Option<String> = None;
            let mut last_preview: Option<String> = None;
            let mut last_shell_cmd: Option<String> = None;
            let mut vscreen = VScreen::new();
            loop {
                match std::io::Read::read(&mut buf_reader, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Prepend any leftover bytes from previous read
                        let mut combined = std::mem::take(&mut utf8_carry);
                        combined.extend_from_slice(&buf[..n]);

                        // Find the last valid UTF-8 boundary
                        let valid_up_to = match std::str::from_utf8(&combined) {
                            Ok(_) => combined.len(),
                            Err(e) => e.valid_up_to(),
                        };

                        let data = String::from_utf8_lossy(&combined[..valid_up_to]).to_string();
                        utf8_carry = combined[valid_up_to..].to_vec();

                        // Feed into virtual screen (only valid UTF-8 portion)
                        vscreen.feed(data.as_bytes());

                        // Record screen state to file (only when recording is enabled)
                        if crate::RECORDING.load(std::sync::atomic::Ordering::Relaxed) {
                            let rec_dir = format!(
                                "{}/.chatterm/recordings",
                                std::env::var("HOME").unwrap_or_default()
                            );
                            std::fs::create_dir_all(&rec_dir).ok();
                            let ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis();
                            use std::io::Write as _;

                            // Raw stream log (for analyzing incremental output)
                            let raw_file = format!("{}/{}.raw.log", rec_dir, session_id);
                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&raw_file)
                            {
                                writeln!(f, "=== {}ms ({} bytes) ===", ts, data.len()).ok();
                                writeln!(f, "{}", data).ok();
                            }

                            // Screen snapshot log
                            let rows = vscreen.rows();
                            if !rows.is_empty() {
                                let rec_file = format!("{}/{}.log", rec_dir, session_id);
                                if let Ok(mut f) = std::fs::OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(&rec_file)
                                {
                                    writeln!(f, "=== {}ms ===", ts).ok();
                                    for (i, row) in rows.iter().enumerate() {
                                        if !row.trim().is_empty() {
                                            writeln!(f, "[{:2}] {}", i, row).ok();
                                        }
                                    }
                                    writeln!(f).ok();
                                }
                            }
                        }

                        // Detect agent + state using config-driven matching
                        let mut agent: Option<String> = None;
                        let mut state: Option<String> = None;
                        let mut title = None;

                        // OSC title detection
                        if let Some(t) = extract_osc_title(&data) {
                            if let Some(cfg) = agent_config::detect_agent(Some(&t), "") {
                                agent = Some(cfg.id.clone());
                                state = cfg.detect_state_from_title(&t).map(|s| s.to_string());
                            }
                            title = Some(t);
                        }

                        // Detect agent from content (always try, allows switching agents)
                        if agent.is_none() {
                            if let Some(cfg) = agent_config::detect_agent(None, &data) {
                                // Only update if different from current
                                if last_agent_cfg.map(|c| c.id.as_str()) != Some(cfg.id.as_str()) {
                                    agent = Some(cfg.id.clone());
                                }
                            }
                        }

                        // Resolve current agent config
                        let cur_cfg = agent
                            .as_deref()
                            .and_then(|id| agent_config::agents().iter().find(|a| a.id == id))
                            .or(last_agent_cfg);

                        // Check for OSC 777/99 notifications
                        let notification = extract_osc777(&data).or_else(|| extract_osc99(&data));
                        let notif_preview = notification.as_ref().map(|n| {
                            if n.body.is_empty() {
                                n.title.clone()
                            } else {
                                n.body.clone()
                            }
                        });

                        // Extract preview:
                        // Normal mode (hooks installed): OSC 777 only
                        // Verbose mode (no hooks): OSC 777 > vscreen scrape fallback
                        let verbose = std::env::var("CHATTERM_VERBOSE").is_ok();
                        let rows = vscreen.rows();
                        let preview = notif_preview.or_else(|| {
                            // Shell sessions (no agent): only update on newline (user pressed Enter)
                            if cur_cfg.is_none() {
                                if !data.contains('\n') && !data.contains('\r') {
                                    return None;
                                }
                                let mut last_dir = None;
                                for row in rows.iter().rev() {
                                    let t = row.trim();
                                    if t.is_empty() {
                                        continue;
                                    }
                                    // Extract dir name from prompt "user@host:~/path$" or "user@host:~/path$ cmd"
                                    let extract_dir = |s: &str| -> Option<String> {
                                        let clean = s.trim_end_matches(['$', '%', ' ']);
                                        clean
                                            .rfind(':')
                                            .map(|i| {
                                                let path = &clean[i + 1..];
                                                path.split('/')
                                                    .next_back()
                                                    .unwrap_or(path)
                                                    .to_string()
                                            })
                                            .filter(|d| !d.is_empty())
                                    };
                                    // Prompt with command
                                    if let Some(pos) = t.rfind("$ ").or_else(|| t.rfind("% ")) {
                                        let cmd_part = t[pos + 2..].trim();
                                        if !cmd_part.is_empty() {
                                            let dir =
                                                extract_dir(&t[..pos + 1]).unwrap_or_default();
                                            let result = format!("{}$ {}", dir, cmd_part);
                                            let result = if result.chars().count() > 50 {
                                                let end = result
                                                    .char_indices()
                                                    .nth(47)
                                                    .map(|(i, _)| i)
                                                    .unwrap_or(result.len());
                                                format!("{}…", &result[..end])
                                            } else {
                                                result
                                            };
                                            last_shell_cmd = Some(result.clone());
                                            return Some(result);
                                        }
                                        if last_dir.is_none() {
                                            last_dir = extract_dir(&t[..pos + 1]);
                                        }
                                        continue;
                                    }
                                    if t.ends_with('$') || t.ends_with('%') {
                                        if last_dir.is_none() {
                                            last_dir = extract_dir(t);
                                        }
                                        continue;
                                    }
                                    break;
                                }
                                return last_shell_cmd
                                    .clone()
                                    .or_else(|| last_dir.map(|d| format!("{}$", d)));
                            }
                            if !verbose {
                                return None;
                            }
                            // Verbose fallback: scan vscreen
                            if let Some(cfg) = cur_cfg {
                                let mut past_input_zone = !cfg.has_input_zone();
                                for row in rows.iter().rev() {
                                    let t = row.trim();
                                    if t.is_empty() || t.len() < 2 {
                                        continue;
                                    }
                                    if !past_input_zone {
                                        if cfg.is_input_zone_boundary(t) {
                                            past_input_zone = true;
                                        }
                                        continue;
                                    }
                                    if t.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
                                        continue;
                                    }
                                    if cfg.is_chrome(t) {
                                        continue;
                                    }
                                    let cleaned = cfg.strip_reply_prefix(t);
                                    if cleaned.is_empty() {
                                        continue;
                                    }
                                    let result = if cleaned.chars().count() > 60 {
                                        let end = cleaned
                                            .char_indices()
                                            .nth(57)
                                            .map(|(i, _)| i)
                                            .unwrap_or(cleaned.len());
                                        format!("{}…", &cleaned[..end])
                                    } else {
                                        cleaned.to_string()
                                    };
                                    return Some(result);
                                }
                            }
                            None
                        });

                        let preview_changed = preview.is_some() && preview != last_preview;
                        if preview_changed {
                            last_preview = preview.clone();
                        }

                        // Detect state: screen detection takes priority (more real-time than OSC title)
                        if let Some(cfg) = cur_cfg {
                            if let Some(screen_state) = cfg.detect_state_from_screen(&rows) {
                                state = Some(screen_state.to_string());
                            }
                        }

                        // Emit metadata
                        let agent_changed = agent.is_some()
                            && agent.as_deref() != last_agent_cfg.map(|c| c.id.as_str());
                        let state_changed = state.is_some() && state != last_state;
                        if agent_changed
                            || state_changed
                            || preview_changed
                            || notification.is_some()
                        {
                            if agent_changed {
                                last_agent_cfg = agent.as_deref().and_then(|id| {
                                    agent_config::agents().iter().find(|a| a.id == id)
                                });
                            }
                            if state_changed {
                                last_state = state.clone();
                            }
                            on_meta(PtyMeta {
                                session_id: session_id.clone(),
                                title,
                                agent: if agent_changed { agent.clone() } else { None },
                                state: if state_changed { state } else { None },
                                preview: if preview_changed { preview } else { None },
                                notification: notification.clone(),
                                command: if agent_changed {
                                    detect_agent_command(agent.as_deref())
                                } else {
                                    None
                                },
                                cwd: if agent_changed {
                                    detect_agent_cwd(agent.as_deref())
                                } else {
                                    None
                                },
                            });
                        }

                        on_output(PtyOutput {
                            session_id: session_id.clone(),
                            data,
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        let master_write = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take writer: {e}"))?;

        let session = PtySession {
            master_write: Arc::new(Mutex::new(master_write)),
            master_pty: Some(pair.master),
            id: req.id.to_string(),
            _child: Some(child),
            _process: None,
        };

        self.sessions
            .lock()
            .unwrap()
            .insert(req.id.to_string(), session);
        Ok(())
    }

    pub fn write_to_session(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(id).ok_or("Session not found")?;
        let mut writer = session.master_write.lock().unwrap();
        writer
            .write_all(data)
            .map_err(|e| format!("Write failed: {e}"))?;
        writer.flush().map_err(|e| format!("Flush failed: {e}"))
    }

    pub fn kill_session(&self, id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(mut session) = sessions.remove(id) {
            if let Some(ref mut c) = session._child {
                c.kill().ok();
            }
            if let Some(ref mut p) = session._process {
                p.kill().ok();
            }
        }
        Ok(())
    }

    pub fn resize_session(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(id).ok_or("Session not found")?;
        if let Some(ref master) = session.master_pty {
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| format!("Resize failed: {e}"))
        } else {
            Ok(()) // claude process sessions don't have a PTY master
        }
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse OSC 777 notification: \033]777;notify;TITLE;BODY\007
fn extract_osc777(data: &str) -> Option<PtyNotification> {
    let marker = "\x1b]777;notify;";
    if let Some(start) = data.find(marker) {
        let rest = &data[start + marker.len()..];
        if let Some(end) = rest.find('\x07') {
            let payload = &rest[..end];
            let parts: Vec<&str> = payload.splitn(2, ';').collect();
            return Some(PtyNotification {
                title: parts.first().unwrap_or(&"").to_string(),
                body: parts.get(1).unwrap_or(&"").to_string(),
                subtitle: None,
            });
        }
    }
    None
}

/// Parse OSC 99 (Kitty) notification: \033]99;...;p=title:TEXT\033\\
/// Simplified: just extract the body payload from the last chunk
fn extract_osc99(data: &str) -> Option<PtyNotification> {
    let marker = "\x1b]99;";
    if let Some(start) = data.find(marker) {
        let rest = &data[start + marker.len()..];
        // Find terminator: either \x07 or \x1b\\
        let end = rest.find('\x07').or_else(|| rest.find("\x1b\\"));
        if let Some(end) = end {
            let payload = &rest[..end];
            // Extract after the last ':'
            if let Some(colon) = payload.rfind(':') {
                return Some(PtyNotification {
                    title: "Agent".to_string(),
                    body: payload[colon + 1..].to_string(),
                    subtitle: None,
                });
            }
        }
    }
    None
}

/// Detect the full command line and cwd of a running agent by scanning processes
fn detect_agent_command(agent: Option<&str>) -> Option<String> {
    let bin = match agent? {
        "claude" => "claude",
        "kiro" => "kiro-cli",
        "codex" => "codex",
        _ => return None,
    };
    let output = std::process::Command::new("ps")
        .args(["-eo", "args"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        let base = trimmed.split('/').next_back().unwrap_or(trimmed);
        if base.starts_with(bin) && !trimmed.contains("hook") && !trimmed.contains("ps ") {
            // Clean up: "node /opt/homebrew/bin/codex --foo" → "codex --foo"
            //           "/Users/x/.local/bin/claude --foo" → "claude --foo"
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let mut start = 0;
            // Skip "node" prefix
            if parts
                .first()
                .map(|p| p.ends_with("node") || p.ends_with("node3"))
                .unwrap_or(false)
            {
                start = 1;
            }
            // Replace full path with just binary name
            if let Some(cmd_part) = parts.get(start) {
                let cmd_base = cmd_part.split('/').next_back().unwrap_or(cmd_part);
                let _args: Vec<&str> = parts[start + 1..]
                    .iter()
                    .filter(|a| !a.starts_with("--session-id") && !a.starts_with("--settings"))
                    .copied()
                    .collect();
                // Also filter out the value after --session-id
                let mut clean_args = Vec::new();
                let mut skip_next = false;
                for a in &parts[start + 1..] {
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    if *a == "--session-id" || *a == "--settings" {
                        skip_next = true;
                        continue;
                    }
                    if a.starts_with("--session-id=") || a.starts_with("--settings=") {
                        continue;
                    }
                    clean_args.push(*a);
                }
                let result = if clean_args.is_empty() {
                    cmd_base.to_string()
                } else {
                    format!("{} {}", cmd_base, clean_args.join(" "))
                };
                return Some(result);
            }
            return Some(trimmed.to_string());
        }
    }
    None
}

fn detect_agent_cwd(agent: Option<&str>) -> Option<String> {
    let bin = match agent? {
        "claude" => "claude",
        "kiro" => "kiro-cli",
        "codex" => "codex",
        _ => return None,
    };
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid,args"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim();
        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }
        let base = parts[1].split('/').next_back().unwrap_or(parts[1]);
        if base.starts_with(bin) && !parts[1].contains("hook") {
            let pid = parts[0].trim();
            if let Some(cwd) = process_cwd(pid) {
                return Some(cwd);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn process_cwd(pid: &str) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
}

#[cfg(not(target_os = "linux"))]
fn process_cwd(pid: &str) -> Option<String> {
    // lsof -a -p PID -d cwd: -a means AND all conditions.
    let lsof = std::process::Command::new("lsof")
        .args(["-a", "-p", pid, "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    let lsof_out = String::from_utf8_lossy(&lsof.stdout);
    for l in lsof_out.lines() {
        if l.starts_with('n') && l.len() > 2 {
            return Some(l[1..].to_string());
        }
    }
    None
}

/// Extract terminal title from OSC 0/2 sequences: \033]0;TITLE\007
fn extract_osc_title(data: &str) -> Option<String> {
    let bytes = data.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == 0x1b
            && bytes[i + 1] == b']'
            && (bytes[i + 2] == b'0' || bytes[i + 2] == b'2')
            && bytes[i + 3] == b';'
        {
            let start = i + 4;
            if let Some(end) = bytes[start..].iter().position(|&b| b == 0x07) {
                return Some(String::from_utf8_lossy(&bytes[start..start + end]).to_string());
            }
        }
        i += 1;
    }
    None
}

/// Detect user's default shell: $SHELL → platform fallback → /bin/bash
fn detect_shell() -> String {
    if let Ok(s) = std::env::var("SHELL") {
        if !s.is_empty() {
            return s;
        }
    }
    if let Some(shell) = platform_shell() {
        return shell;
    }
    "/bin/bash".to_string()
}

#[cfg(target_os = "macos")]
fn platform_shell() -> Option<String> {
    let out = std::process::Command::new("dscl")
        .args([".", "-read", &format!("/Users/{}", whoami()), "UserShell"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let shell = s.split_whitespace().next_back()?;
    std::path::Path::new(shell)
        .exists()
        .then(|| shell.to_string())
}

#[cfg(not(target_os = "macos"))]
fn platform_shell() -> Option<String> {
    None
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| {
        String::from_utf8_lossy(
            &std::process::Command::new("whoami")
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default(),
        )
        .trim()
        .to_string()
    })
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| {
        #[cfg(target_os = "macos")]
        {
            format!("/Users/{}", whoami())
        }
        #[cfg(not(target_os = "macos"))]
        {
            format!("/home/{}", whoami())
        }
    })
}

fn expand_path(_home: &str, path: &str) -> String {
    let mut prefixes = Vec::new();
    #[cfg(target_os = "macos")]
    {
        prefixes.push("/opt/homebrew/bin".to_string());
        prefixes.push("/usr/local/bin".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        prefixes.push(format!("{_home}/.local/bin"));
        prefixes.push("/usr/local/bin".to_string());
    }

    let mut merged = Vec::new();
    for prefix in prefixes {
        if !path.split(':').any(|entry| entry == prefix) {
            merged.push(prefix);
        }
    }
    merged.push(path.to_string());
    merged.join(":")
}

// Wrapper to implement portable_pty::Child for std::process::Child
// end of module
