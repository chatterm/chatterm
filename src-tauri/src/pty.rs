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
    /// PID of the spawned shell/agent process. Used by `session_cwd()` so the
    /// frontend can pull the current working directory on demand rather than
    /// relying on a pty-meta emit racing with its listener registration.
    child_pid: u32,
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
        #[cfg(not(windows))]
        cmd.arg("-l");
        if let Some(c) = req.command {
            #[cfg(not(windows))]
            cmd.args(["-c", c]);
            #[cfg(windows)]
            {
                // PowerShell uses -Command, cmd.exe uses /C
                if shell.to_lowercase().contains("powershell") || shell.to_lowercase().contains("pwsh") {
                    cmd.args(["-Command", c]);
                } else {
                    cmd.args(["/C", c]);
                }
            }
        }

        // Set HOME and common env
        let home = home_dir();
        #[cfg(not(windows))]
        cmd.env("HOME", &home);
        #[cfg(windows)]
        cmd.env("USERPROFILE", &home);
        let work_dir = req.cwd.unwrap_or(&home);
        cmd.cwd(work_dir);
        #[cfg(not(windows))]
        cmd.env("USER", whoami());
        #[cfg(windows)]
        cmd.env("USERNAME", whoami());
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", expand_path(&home, &path));
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("CHATTERM_SESSION_ID", req.id);
        // Pass locale through so `less`, git, etc. render multibyte characters
        // (CJK names, filenames) instead of escaping bytes as <hex>. GUI apps
        // launched by launchd inherit a bare environment without LANG.
        for var in ["LANG", "LC_ALL", "LC_CTYPE"] {
            if let Ok(v) = std::env::var(var) {
                cmd.env(var, v);
            }
        }
        if std::env::var("LANG").is_err() && std::env::var("LC_ALL").is_err() {
            cmd.env("LANG", "en_US.UTF-8");
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn: {e}"))?;
        let child_pid = child.process_id().unwrap_or(0);

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
            // Live cwd tracking for shell sessions (agents get their cwd from
            // detect_agent_cwd on agent change). Throttled to avoid spawning
            // lsof / reading /proc on every single PTY output burst.
            let mut last_shell_cwd: Option<String> = None;
            let mut last_cwd_probe = std::time::Instant::now()
                - std::time::Duration::from_secs(10);
            let mut vscreen = VScreen::new();

            // Best-effort initial push of cwd. A single emit can race with the
            // frontend's `listen("pty-meta")` registration on cold start, so
            // the frontend also pulls via the `session_cwd` command after it
            // knows the session exists. The in-loop probe below keeps cwd
            // fresh as the user cds around.
            if child_pid > 0 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Some(cwd) = process_cwd(&child_pid.to_string()) {
                    last_shell_cwd = Some(cwd.clone());
                    last_cwd_probe = std::time::Instant::now();
                    on_meta(PtyMeta {
                        session_id: session_id.clone(),
                        title: None,
                        agent: None,
                        state: None,
                        preview: None,
                        notification: None,
                        command: None,
                        cwd: Some(cwd),
                    });
                }
            }
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
                            let rec_dir_path = crate::chatterm_dir().join("recordings");
                            let rec_dir = rec_dir_path.to_string_lossy().to_string();
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
                                    detect_agent_command(agent.as_deref(), child_pid)
                                } else {
                                    None
                                },
                                cwd: if agent_changed {
                                    detect_agent_cwd(agent.as_deref(), child_pid)
                                } else {
                                    None
                                },
                            });
                        }

                        // Live shell cwd tracking — only for non-agent sessions
                        // (agents have their own cwd discovery via detect_agent_cwd).
                        // Throttled to once per 500ms worth of output bursts.
                        if last_agent_cfg.is_none()
                            && child_pid > 0
                            && last_cwd_probe.elapsed() >= std::time::Duration::from_millis(500)
                        {
                            last_cwd_probe = std::time::Instant::now();
                            if let Some(new_cwd) = process_cwd(&child_pid.to_string()) {
                                if Some(&new_cwd) != last_shell_cwd.as_ref() {
                                    last_shell_cwd = Some(new_cwd.clone());
                                    on_meta(PtyMeta {
                                        session_id: session_id.clone(),
                                        title: None,
                                        agent: None,
                                        state: None,
                                        preview: None,
                                        notification: None,
                                        command: None,
                                        cwd: Some(new_cwd),
                                    });
                                }
                            }
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
            child_pid,
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

    /// Look up the current working directory of a session's child process.
    /// Called by the frontend via the `session_cwd` Tauri command to pull
    /// the cwd on demand (avoids racing with `pty-meta` event listener setup).
    pub fn session_cwd(&self, id: &str) -> Option<String> {
        let sessions = self.sessions.lock().ok()?;
        let session = sessions.get(id)?;
        if session.child_pid == 0 {
            return None;
        }
        process_cwd(&session.child_pid.to_string())
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

/// Return the set of PIDs that are transitive descendants of `root_pid`
/// (not including `root_pid` itself). Used to scope agent detection to the
/// session's own process subtree so unrelated instances of the same binary
/// running elsewhere on the machine are not mis-attributed. Returns an empty
/// set if `root_pid` is 0 or `ps` fails.
#[cfg(not(windows))]
fn descendants_of(root_pid: u32) -> std::collections::HashSet<u32> {
    let mut descendants = std::collections::HashSet::new();
    if root_pid == 0 {
        return descendants;
    }
    let output = match std::process::Command::new("ps")
        .args(["-eo", "pid,ppid"])
        .output()
        .ok()
    {
        Some(o) => o,
        None => return descendants,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(pid) = parts[0].parse::<u32>() else { continue };
        let Ok(ppid) = parts[1].parse::<u32>() else { continue };
        children.entry(ppid).or_default().push(pid);
    }
    let mut queue = vec![root_pid];
    while let Some(pid) = queue.pop() {
        if let Some(kids) = children.get(&pid) {
            for &kid in kids {
                if descendants.insert(kid) {
                    queue.push(kid);
                }
            }
        }
    }
    descendants
}

#[cfg(windows)]
fn descendants_of(_root_pid: u32) -> std::collections::HashSet<u32> {
    std::collections::HashSet::new() // TODO: implement via CreateToolhelp32Snapshot
}
/// Detect the full command line and cwd of a running agent by scanning the
/// descendants of `root_pid` (the session's own PTY child). Unrelated
/// instances of the same binary elsewhere on the system are ignored.
#[cfg(not(windows))]
fn detect_agent_command(agent: Option<&str>, root_pid: u32) -> Option<String> {
    let bin = match agent? {
        "claude" => "claude",
        "kiro" => "kiro-cli",
        "codex" => "codex",
        _ => return None,
    };
    let tree = descendants_of(root_pid);
    if tree.is_empty() {
        return None;
    }
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid,args"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        let (pid_s, args) = match trimmed.split_once(char::is_whitespace) {
            Some((p, a)) => (p.trim(), a.trim()),
            None => continue,
        };
        let Ok(pid) = pid_s.parse::<u32>() else { continue };
        if !tree.contains(&pid) {
            continue;
        }
        let base = args.split('/').next_back().unwrap_or(args);
        if base.starts_with(bin) && !args.contains("hook") && !args.contains("ps ") {
            let trimmed = args;
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

#[cfg(not(windows))]
fn detect_agent_cwd(agent: Option<&str>, root_pid: u32) -> Option<String> {
    let bin = match agent? {
        "claude" => "claude",
        "kiro" => "kiro-cli",
        "codex" => "codex",
        _ => return None,
    };
    let tree = descendants_of(root_pid);
    if tree.is_empty() {
        return None;
    }
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid,args"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(pid) = parts[0].parse::<u32>() else { continue };
        if !tree.contains(&pid) {
            continue;
        }
        let base = parts[1].split('/').next_back().unwrap_or(parts[1]);
        if base.starts_with(bin) && !parts[1].contains("hook") {
            if let Some(cwd) = process_cwd(&pid.to_string()) {
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

#[cfg(target_os = "macos")]
fn process_cwd(pid: &str) -> Option<String> {
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

#[cfg(windows)]
fn process_cwd(_pid: &str) -> Option<String> {
    None // TODO: implement via NtQueryInformationProcess
}

#[cfg(windows)]
fn detect_agent_command(_agent: Option<&str>, _root_pid: u32) -> Option<String> { None }

#[cfg(windows)]
fn detect_agent_cwd(_agent: Option<&str>, _root_pid: u32) -> Option<String> { None }

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
    #[cfg(windows)]
    { "powershell.exe".to_string() }
    #[cfg(not(windows))]
    { "/bin/bash".to_string() }
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

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_shell() -> Option<String> {
    None
}

#[cfg(windows)]
fn platform_shell() -> Option<String> {
    // Prefer pwsh (PowerShell 7+) if available, else fall back to powershell.exe
    if let Ok(o) = std::process::Command::new("where").arg("pwsh.exe").output() {
        let s = String::from_utf8_lossy(&o.stdout);
        let p = s.trim();
        if !p.is_empty() { return Some(p.to_string()); }
    }
    None
}

fn whoami() -> String {
    #[cfg(windows)]
    { std::env::var("USERNAME").unwrap_or_else(|_| "user".into()) }
    #[cfg(not(windows))]
    {
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
}

fn home_dir() -> String {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").unwrap_or_else(|_| {
            format!("C:\\Users\\{}", whoami())
        })
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").unwrap_or_else(|_| {
            #[cfg(target_os = "macos")]
            { format!("/Users/{}", whoami()) }
            #[cfg(not(target_os = "macos"))]
            { format!("/home/{}", whoami()) }
        })
    }
}

fn expand_path(_home: &str, path: &str) -> String {
    #[cfg(windows)]
    { path.to_string() }
    #[cfg(not(windows))]
    {
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
}

// Wrapper to implement portable_pty::Child for std::process::Child
// end of module
