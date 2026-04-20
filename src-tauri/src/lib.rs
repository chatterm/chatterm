mod pty;
pub mod vscreen;
pub mod agent_config;
pub mod theme;
pub mod session;

use pty::{PtyManager, PtyMeta, PtyOutput};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

static RECORDING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn toggle_recording() -> bool {
    let was = RECORDING.load(Ordering::Relaxed);
    let now = !was;
    RECORDING.store(now, Ordering::Relaxed);
    if now {
        let dir = format!("{}/.chatterm/recordings", std::env::var("HOME").unwrap_or_default());
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).ok();
    }
    now
}

#[tauri::command]
fn is_recording() -> bool { RECORDING.load(Ordering::Relaxed) }

#[tauri::command]
fn import_terminal_theme(path: String) -> Result<theme::ThemeColors, String> {
    theme::parse_terminal_file(&path)
}

#[tauri::command]
fn list_system_terminal_themes() -> Vec<String> {
    theme::list_system_themes()
}

#[tauri::command]
fn export_system_terminal_theme(name: String) -> Result<theme::ThemeColors, String> {
    theme::export_system_theme(&name)
}

struct AppState { pty_mgr: Arc<PtyManager> }

#[tauri::command]
fn save_sessions(sessions: Vec<session::SessionMeta>) {
    session::save(&sessions);
}

#[tauri::command]
fn load_sessions() -> Vec<session::SessionMeta> {
    session::load()
}

#[tauri::command]
fn create_session(app: AppHandle, state: tauri::State<'_, AppState>, id: String, cols: u16, rows: u16, command: Option<String>, cwd: Option<String>) -> Result<(), String> {
    let h1 = app.clone();
    let h2 = app.clone();
    state.pty_mgr.create_session(&id, cols, rows, command.as_deref(), cwd.as_deref(),
        move |o: PtyOutput| { h1.emit("pty-output", &o).ok(); },
        move |m: PtyMeta| { h2.emit("pty-meta", &m).ok(); },
    )
}

#[tauri::command]
fn write_session(state: tauri::State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    state.pty_mgr.write_to_session(&id, data.as_bytes())
}

#[tauri::command]
fn kill_session(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state.pty_mgr.kill_session(&id)
}

#[tauri::command]
fn resize_session(state: tauri::State<'_, AppState>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    state.pty_mgr.resize_session(&id, cols, rows)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { pty_mgr: Arc::new(PtyManager::new()) })
        .invoke_handler(tauri::generate_handler![
            create_session, write_session, kill_session, resize_session,
            toggle_recording, is_recording,
            import_terminal_theme, list_system_terminal_themes, export_system_terminal_theme,
            save_sessions, load_sessions,
        ])
        .setup(|app| {
            start_fifo_listener(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// FIFO IPC: hook scripts write JSON lines to this pipe, we emit pty-meta events
fn start_fifo_listener(app: AppHandle) {
    use std::io::{BufRead, BufReader};

    let pipe_path = format!("{}/.chatterm/hook.pipe", std::env::var("HOME").unwrap_or_default());
    let dir = format!("{}/.chatterm", std::env::var("HOME").unwrap_or_default());
    std::fs::create_dir_all(&dir).ok();

    // Remove stale pipe, create fresh FIFO
    std::fs::remove_file(&pipe_path).ok();
    unsafe { libc::mkfifo(std::ffi::CString::new(pipe_path.as_str()).unwrap().as_ptr(), 0o622); }

    std::thread::spawn(move || {
        loop {
            // open blocks until a writer connects; re-open after EOF to wait for next writer
            let file = match std::fs::File::open(&pipe_path) {
                Ok(f) => f,
                Err(_) => { std::thread::sleep(std::time::Duration::from_millis(500)); continue; }
            };
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = match line { Ok(l) => l, Err(_) => break };
                if line.trim().is_empty() { continue; }
                // Parse: {"session_id":"s0","type":"reply","body":"..."}
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                    let sid = msg.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let body = msg.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let msg_cwd = msg.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
                    if !body.is_empty() {
                        let (state, preview) = match msg_type {
                            "reply"  => (None, Some(body)),
                            "done"   => (Some("idle".to_string()), None),
                            "ask"    => (Some("idle".to_string()), None),
                            "tool"   => (Some("thinking".to_string()), None),
                            _        => (None, None),
                        };
                        if state.is_some() || preview.is_some() || msg_cwd.is_some() {
                            app.emit("pty-meta", &PtyMeta {
                                session_id: sid,
                                title: None,
                                agent: None,
                                state,
                                preview,
                                notification: None,
                                command: None,
                                cwd: msg_cwd,
                            }).ok();
                        }
                    }
                }
            }
            // Writer closed, loop back to re-open
        }
    });
}
