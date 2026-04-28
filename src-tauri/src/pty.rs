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
        #[cfg(windows)]
        {
            // PowerShell 5.x does not update the terminal title on cd, and
            // does not call SetCurrentDirectory (so process_cwd() is stale).
            // Inject an OSC 7 prompt hook so CWD tracking works out of the box.
            if req.command.is_none()
                && (shell.to_lowercase().contains("powershell")
                    || shell.to_lowercase().contains("pwsh"))
            {
                cmd.args(["-NoExit", "-Command",
                    // Prepend an OSC 7 emitter to the existing prompt function.
                    // Emit OSC 7 before every prompt for CWD tracking.
                    // Uses $([char]27) for PS 5.x compat. The prompt function
                    // emits OSC 7 then returns the default "PS path> " prompt.
                    // If $PROFILE redefines prompt after this, OSC 7 is lost —
                    // but the OSC 0 title fallback (extract_windows_path_from_title)
                    // still provides CWD tracking for pwsh 7+ which sets title.
                    r#"function prompt { $p = $PWD.Path -replace '\\','/'; [Console]::Write($([char]27) + "]7;file://" + $env:COMPUTERNAME + "/" + $p + $([char]27) + "\"); return "PS $($PWD.Path)> " }"#,
                ]);
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
            // When OSC 7 reports a remote hostname, disable the /proc-based
            // fallback probe — it would read the local `ssh` process's cwd
            // and overwrite the correct remote path.
            let mut cwd_via_remote_osc7 = false;
            // When ANY OSC 7 has been received (local or remote), the shell
            // is emitting CWD via OSC 7. Use a timestamp so the suppression
            // self-heals: if the shell stops emitting OSC 7 (e.g. user runs
            // cmd.exe inside PowerShell), process_cwd() resumes after 10s.
            let mut last_osc7_time: Option<std::time::Instant> = None;
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

                        // OSC 7 CWD tracking (works across SSH — remote shell
                        // sends file://hostname/path which tunnels back through PTY)
                        if let Some((host, path)) = extract_osc7(&data) {
                            let local = host.is_empty()
                                || host == "localhost"
                                || is_local_hostname(&host);
                            let new_cwd = if local {
                                path
                            } else {
                                // Wrap bare IPv6 addresses in brackets so the
                                // frontend regex can distinguish host from path
                                let display_host = if host.contains(':') {
                                    format!("[{}]", host)
                                } else {
                                    host
                                };
                                format!("{}:{}", display_host, path)
                            };
                            cwd_via_remote_osc7 = !local;
                            last_osc7_time = Some(std::time::Instant::now());
                            if Some(&new_cwd) != last_shell_cwd.as_ref() {
                                last_shell_cwd = Some(new_cwd.clone());
                                last_cwd_probe = std::time::Instant::now();
                                on_meta(PtyMeta {
                                    session_id: session_id.clone(),
                                    title: None, agent: None, state: None,
                                    preview: None, notification: None, command: None,
                                    cwd: Some(new_cwd),
                                });
                            }
                        }

                        // Fallback CWD from OSC 0 title — PowerShell sets the
                        // terminal title to the current directory (e.g. "C:\Users\foo"
                        // or "PS C:\Users\foo>") but does NOT call SetCurrentDirectory,
                        // so process_cwd() returns a stale path. Extract a Windows
                        // path from the title as a fallback when no OSC 7 is active.
                        if !cwd_via_remote_osc7 && last_agent_cfg.is_none() {
                            if let Some(ref t) = title {
                                if let Some(cwd) = extract_windows_path_from_title(t) {
                                    if Some(&cwd) != last_shell_cwd.as_ref() {
                                        last_shell_cwd = Some(cwd.clone());
                                        on_meta(PtyMeta {
                                            session_id: session_id.clone(),
                                            title: None, agent: None, state: None,
                                            preview: None, notification: None,
                                            command: None, cwd: Some(cwd),
                                        });
                                    }
                                }
                            }
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
                                // Suppress when a TUI app (nvim, vim, htop…) is using
                                // the alternate screen — its status bar content would be
                                // misinterpreted as a shell prompt.
                                if vscreen.is_alt_screen() {
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

                        // Live shell cwd tracking — only for non-agent sessions.
                        // Throttled to once per 500ms worth of output bursts.
                        // Skip entirely while OSC 7 is the CWD source — either
                        // from a remote SSH session or a local shell with OSC 7
                        // prompt (e.g. our injected PowerShell prompt). On Windows
                        // process_cwd() reads a stale PEB that would overwrite
                        // the correct OSC 7 path.
                        if last_agent_cfg.is_none()
                            && child_pid > 0
                            && !cwd_via_remote_osc7
                            && last_osc7_time.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(10))
                            && last_cwd_probe.elapsed() >= std::time::Duration::from_millis(500)
                        {
                            last_cwd_probe = std::time::Instant::now();
                            if let Some(new_cwd) = process_cwd(&child_pid.to_string()) {
                                if Some(&new_cwd) != last_shell_cwd.as_ref() {
                                    last_shell_cwd = Some(new_cwd.clone());
                                    on_meta(PtyMeta {
                                        session_id: session_id.clone(),
                                        title: None, agent: None, state: None,
                                        preview: None, notification: None,
                                        command: None, cwd: Some(new_cwd),
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

/// Parse OSC 7 (shell CWD reporting): \033]7;file://HOSTNAME/PATH\007 or \033]7;...;\033\\
/// Returns (hostname, path). Used to track CWD across SSH sessions where
/// /proc or lsof cannot reach the remote shell.
///
/// NOTE: Only the `file://` scheme is handled. Some terminals emit bare paths
/// (`\033]7;/home/user\007`) without the scheme — those are intentionally
/// ignored and fall through to the /proc-based fallback.
fn extract_osc7(data: &str) -> Option<(String, String)> {
    let marker = "\x1b]7;file://";
    // Use rfind so that if multiple OSC 7 sequences arrive in one read
    // (e.g. rapid `cd` commands), we pick the last (most recent) one.
    let start = data.rfind(marker)? + marker.len();
    let rest = &data[start..];
    let end = rest.find('\x07').or_else(|| rest.find("\x1b\\"))?;
    let payload = &rest[..end];
    let slash = payload.find('/')?;
    let host = percent_decode(&payload[..slash]);
    let path = percent_decode(&payload[slash..]);
    // Basic path validation: reject traversal attempts (e.g. /../../../etc)
    if path.split('/').any(|seg| seg == "..") {
        return None;
    }
    Some((host, path))
}

/// Decode percent-encoded bytes in a URI path (RFC 3986).
/// Handles `%20` → space, `%E4%BD%A0` → 你, etc.
fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
#[allow(dead_code)]
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
fn process_cwd(pid: &str) -> Option<String> {
    // Only x64 PEB offsets are defined; reject 32-bit at compile time.
    #[cfg(not(target_pointer_width = "64"))]
    compile_error!("Windows process_cwd() only supports 64-bit targets");

    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let pid_u32: u32 = pid.parse().ok()?;

    // RAII wrapper so the handle is closed even if read_process_cwd panics.
    struct OwnedHandle(windows::Win32::Foundation::HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.0); }
        }
    }

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            pid_u32,
        ).ok()?
    };
    let owned = OwnedHandle(handle);
    unsafe { read_process_cwd(owned.0) }
}

/// Cached NtQueryInformationProcess function pointer (loaded once from ntdll).
#[cfg(windows)]
type NtQueryFn = unsafe extern "system" fn(
    handle: isize,   // HANDLE
    class: u32,      // PROCESSINFOCLASS
    info: *mut u8,   // output buffer
    len: u32,        // buffer size
    ret_len: *mut u32,
) -> i32; // NTSTATUS

#[cfg(windows)]
fn cached_nt_query() -> Option<NtQueryFn> {
    use std::sync::OnceLock;
    static FUNC: OnceLock<Option<NtQueryFn>> = OnceLock::new();
    *FUNC.get_or_init(|| unsafe {
        // ntdll.dll is always loaded in every Windows process — use
        // GetModuleHandleA (no refcount bump) instead of LoadLibraryA.
        let ntdll = windows::Win32::System::LibraryLoader::GetModuleHandleA(
            windows::core::PCSTR(c"ntdll.dll".as_ptr() as _),
        ).ok()?;
        let func = windows::Win32::System::LibraryLoader::GetProcAddress(
            ntdll,
            windows::core::PCSTR(c"NtQueryInformationProcess".as_ptr() as _),
        )?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, NtQueryFn>(func))
    })
}

/// Read CWD from a remote process via NtQueryInformationProcess → PEB → ProcessParameters.
///
/// x64 layout only — guarded by compile_error! in `process_cwd()`.
#[cfg(windows)]
unsafe fn read_process_cwd(handle: windows::Win32::Foundation::HANDLE) -> Option<String> {
    let nt_query = cached_nt_query()?;

    // PROCESS_BASIC_INFORMATION (x64):
    //   NTSTATUS ExitStatus       (i32 + 4 bytes padding)
    //   PPEB     PebBaseAddress   (usize)
    //   ULONG_PTR AffinityMask    (usize)
    //   KPRIORITY BasePriority    (i32 + 4 bytes padding)
    //   ULONG_PTR UniqueProcessId (usize)
    //   ULONG_PTR InheritedFromUniqueProcessId (usize)
    #[repr(C)]
    struct ProcessBasicInformation {
        exit_status: i32,
        _pad0: u32,
        peb_base_address: usize,
        affinity_mask: usize,
        base_priority: i32,
        _pad1: u32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }
    const _: () = assert!(std::mem::size_of::<ProcessBasicInformation>() == 48);

    let mut pbi = std::mem::zeroed::<ProcessBasicInformation>();
    // .0 accesses the raw isize inside windows::Win32::Foundation::HANDLE
    let status = nt_query(
        handle.0 as isize,                                  // process handle
        0,                                                   // ProcessBasicInformation
        &mut pbi as *mut _ as *mut u8,                       // output buffer
        std::mem::size_of::<ProcessBasicInformation>() as u32, // buffer size
        std::ptr::null_mut(),                                // optional return length
    );
    if status < 0 || pbi.peb_base_address == 0 {
        return None;
    }

    // PEB.ProcessParameters pointer: offset 0x20 on x64
    let mut params_ptr: usize = 0;
    read_mem(handle, pbi.peb_base_address + 0x20, &mut params_ptr)?;
    if params_ptr == 0 {
        return None;
    }

    // RTL_USER_PROCESS_PARAMETERS.CurrentDirectory is a CURDIR at offset 0x38 (x64).
    // CURDIR starts with a UNICODE_STRING: { Length: u16, MaxLength: u16, pad: u32, Buffer: usize }
    let mut len: u16 = 0;
    read_mem(handle, params_ptr + 0x38, &mut len)?;
    // Windows long-path max is 32767 chars = 65534 bytes
    if len == 0 || len as usize > 32767 * 2 {
        return None;
    }

    // Buffer pointer at +8 bytes from UNICODE_STRING start (after Length + MaxLength + padding)
    let mut buf_ptr: usize = 0;
    read_mem(handle, params_ptr + 0x38 + 8, &mut buf_ptr)?;
    if buf_ptr == 0 {
        return None;
    }

    let char_count = len as usize / 2;
    let mut buf = vec![0u16; char_count];
    let mut bytes_read = 0usize;
    let ok = windows::Win32::System::Diagnostics::Debug::ReadProcessMemory(
        handle,
        buf_ptr as *const _,
        buf.as_mut_ptr() as *mut _,
        char_count * 2,
        Some(&mut bytes_read),
    );
    if ok.is_err() || bytes_read < char_count * 2 {
        return None;
    }

    let s = String::from_utf16_lossy(&buf);
    // Strip trailing backslash (e.g. "C:\Users\foo\" → "C:\Users\foo")
    Some(s.trim_end_matches('\\').to_string())
}

#[cfg(windows)]
unsafe fn read_mem<T: Copy>(
    handle: windows::Win32::Foundation::HANDLE,
    addr: usize,
    out: &mut T,
) -> Option<()> {
    let mut read = 0usize;
    windows::Win32::System::Diagnostics::Debug::ReadProcessMemory(
        handle,
        addr as *const _,
        out as *mut T as *mut _,
        std::mem::size_of::<T>(),
        Some(&mut read),
    )
    .ok()?;
    if read != std::mem::size_of::<T>() {
        return None;
    }
    Some(())
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

/// Extract a Windows directory path from a terminal title string.
///
/// PowerShell sets the title to the CWD but does NOT call SetCurrentDirectory,
/// so process_cwd() returns a stale path. Common title formats:
///   "C:\Users\foo"
///   "PS C:\Users\foo>"
///   "Administrator: C:\Windows\System32"
///   "Windows PowerShell"  (no path — skip)
fn extract_windows_path_from_title(title: &str) -> Option<String> {
    // Match a drive-letter path (X:\...) that appears at the start of the
    // title or after a known prefix ("PS ", "Administrator: "). This avoids
    // false-matching titles like "vim C:\temp\file.txt".
    // UNC paths (\\server\share) are not handled — worth a follow-up.
    let t = title.trim();
    // Strip known prefixes to find the path at the "start"
    let stripped = t
        .strip_prefix("Administrator: ")
        .or_else(|| t.strip_prefix("PS "))
        .unwrap_or(t);
    let bytes = stripped.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
    {
        // Extract to end of path (stop at '>' or end of string)
        let path_end = bytes
            .iter()
            .position(|&c| c == b'>')
            .unwrap_or(bytes.len());
        let path = stripped[..path_end].trim().trim_end_matches('\\');
        if !path.is_empty() {
            return Some(path.to_string());
        }
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

/// Check if a hostname matches the local machine, avoiding false SSH detection
/// when the local shell's OSC 7 uses the machine's actual hostname.
fn is_local_hostname(host: &str) -> bool {
    use std::sync::OnceLock;
    static LOCAL: OnceLock<String> = OnceLock::new();
    let local = LOCAL.get_or_init(|| {
        #[cfg(unix)]
        {
            let mut buf = [0u8; 256];
            unsafe {
                if libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) == 0 {
                    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                    return String::from_utf8_lossy(&buf[..len]).into_owned();
                }
            }
        }
        #[cfg(windows)]
        {
            if let Ok(h) = std::env::var("COMPUTERNAME") {
                return h;
            }
        }
        String::new()
    });
    !local.is_empty() && host.eq_ignore_ascii_case(local.as_str())
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

#[cfg(test)]
mod pty_tests {
    use super::*;

    // --- extract_osc7 ---

    #[test]
    fn osc7_basic_remote() {
        let data = "\x1b]7;file://ubuntu22/home/user\x07";
        let (host, path) = extract_osc7(data).unwrap();
        assert_eq!(host, "ubuntu22");
        assert_eq!(path, "/home/user");
    }

    #[test]
    fn osc7_localhost() {
        let data = "\x1b]7;file://localhost/tmp\x07";
        let (host, path) = extract_osc7(data).unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(path, "/tmp");
    }

    #[test]
    fn osc7_empty_host() {
        let data = "\x1b]7;file:///home/user\x07";
        let (host, path) = extract_osc7(data).unwrap();
        assert_eq!(host, "");
        assert_eq!(path, "/home/user");
    }

    #[test]
    fn osc7_percent_encoded_path() {
        let data = "\x1b]7;file://host/home/my%20dir\x07";
        let (_, path) = extract_osc7(data).unwrap();
        assert_eq!(path, "/home/my dir");
    }

    #[test]
    fn osc7_windows_forward_slash() {
        // PowerShell OSC 7 injection sends forward slashes
        let data = "\x1b]7;file://DESKTOP-ABC/C:/Users/foo\x07";
        let (host, path) = extract_osc7(data).unwrap();
        assert_eq!(host, "DESKTOP-ABC");
        assert_eq!(path, "/C:/Users/foo");
    }

    #[test]
    fn osc7_rejects_traversal() {
        let data = "\x1b]7;file://host/home/../../etc/passwd\x07";
        assert!(extract_osc7(data).is_none());
    }

    #[test]
    fn osc7_st_terminator() {
        // ESC \ (ST) instead of BEL
        let data = "\x1b]7;file://host/path\x1b\\";
        let (host, path) = extract_osc7(data).unwrap();
        assert_eq!(host, "host");
        assert_eq!(path, "/path");
    }

    #[test]
    fn osc7_picks_last_in_burst() {
        let data = "\x1b]7;file://h/old\x07some output\x1b]7;file://h/new\x07";
        let (_, path) = extract_osc7(data).unwrap();
        assert_eq!(path, "/new");
    }

    // --- extract_osc_title ---

    #[test]
    fn osc_title_basic() {
        let data = "\x1b]0;My Title\x07";
        assert_eq!(extract_osc_title(data).unwrap(), "My Title");
    }

    #[test]
    fn osc_title_type2() {
        let data = "\x1b]2;Window Title\x07";
        assert_eq!(extract_osc_title(data).unwrap(), "Window Title");
    }

    #[test]
    fn osc_title_no_match() {
        assert!(extract_osc_title("plain text").is_none());
    }

    #[test]
    fn osc_title_powershell_path() {
        let data = "\x1b]0;C:\\Users\\foo\x07";
        assert_eq!(extract_osc_title(data).unwrap(), "C:\\Users\\foo");
    }

    // --- extract_windows_path_from_title ---

    #[test]
    fn win_title_bare_path() {
        assert_eq!(
            extract_windows_path_from_title("C:\\Users\\foo"),
            Some("C:\\Users\\foo".into())
        );
    }

    #[test]
    fn win_title_ps_prefix() {
        assert_eq!(
            extract_windows_path_from_title("PS C:\\Users\\foo>"),
            Some("C:\\Users\\foo".into())
        );
    }

    #[test]
    fn win_title_admin_prefix() {
        assert_eq!(
            extract_windows_path_from_title("Administrator: C:\\Windows\\System32"),
            Some("C:\\Windows\\System32".into())
        );
    }

    #[test]
    fn win_title_trailing_backslash() {
        assert_eq!(
            extract_windows_path_from_title("C:\\"),
            Some("C:".into())
        );
    }

    #[test]
    fn win_title_no_path() {
        assert!(extract_windows_path_from_title("Windows PowerShell").is_none());
    }

    #[test]
    fn win_title_embedded_path_rejected() {
        // "vim C:\temp\file" should NOT match — path not at start
        assert!(extract_windows_path_from_title("vim C:\\temp\\file").is_none());
    }

    // --- percent_decode ---

    #[test]
    fn percent_decode_space() {
        assert_eq!(percent_decode("/my%20dir"), "/my dir");
    }

    #[test]
    fn percent_decode_utf8() {
        // 你 = E4 BD A0
        assert_eq!(percent_decode("/%E4%BD%A0"), "/你");
    }

    #[test]
    fn percent_decode_passthrough() {
        assert_eq!(percent_decode("/plain/path"), "/plain/path");
    }

    // --- SSH regex (frontend equivalent) ---

    #[test]
    fn ssh_regex_rejects_drive_letter() {
        let re = regex::Regex::new(r"^(\[[^\]]+\]|[^/:]{2,}):(/.*$)").unwrap();
        assert!(!re.is_match("C:/Users/foo"), "single-letter host should not match");
        assert!(re.is_match("ubuntu22:/home/user"));
        assert!(re.is_match("[::1]:/tmp"));
    }
}