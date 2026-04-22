pub mod agent_config;
mod pty;
pub mod session;
pub mod theme;
pub mod vscreen;

use pty::{CreateSessionRequest, PtyManager, PtyMeta, PtyOutput};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

static RECORDING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn toggle_recording() -> bool {
    let was = RECORDING.load(Ordering::Relaxed);
    let now = !was;
    RECORDING.store(now, Ordering::Relaxed);
    if now {
        let dir = chatterm_dir().join("recordings");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).ok();
    }
    now
}

#[tauri::command]
fn is_recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

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

struct AppState {
    pty_mgr: Arc<PtyManager>,
}

#[tauri::command]
fn save_sessions(sessions: Vec<session::SessionMeta>) {
    session::save(&sessions);
}

#[tauri::command]
fn load_sessions() -> Vec<session::SessionMeta> {
    session::load()
}

#[tauri::command]
fn create_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    command: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let h1 = app.clone();
    let h2 = app.clone();
    let req = CreateSessionRequest {
        id: &id,
        cols,
        rows,
        command: command.as_deref(),
        cwd: cwd.as_deref(),
    };
    state.pty_mgr.create_session(
        req,
        move |o: PtyOutput| {
            h1.emit("pty-output", &o).ok();
        },
        move |m: PtyMeta| {
            h2.emit("pty-meta", &m).ok();
        },
    )
}

#[tauri::command]
fn write_session(
    state: tauri::State<'_, AppState>,
    id: String,
    data: String,
) -> Result<(), String> {
    state.pty_mgr.write_to_session(&id, data.as_bytes())
}

#[tauri::command]
fn kill_session(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state.pty_mgr.kill_session(&id)
}

#[tauri::command]
fn resize_session(
    state: tauri::State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.pty_mgr.resize_session(&id, cols, rows)
}

#[tauri::command]
fn session_cwd(state: tauri::State<'_, AppState>, id: String) -> Option<String> {
    state.pty_mgr.session_cwd(&id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            pty_mgr: Arc::new(PtyManager::new()),
        })
        .invoke_handler(tauri::generate_handler![
            create_session,
            write_session,
            kill_session,
            resize_session,
            session_cwd,
            toggle_recording,
            is_recording,
            import_terminal_theme,
            list_system_terminal_themes,
            export_system_terminal_theme,
            save_sessions,
            load_sessions,
        ])
        .setup(|app| {
            start_fifo_listener(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Cross-platform chatterm data directory
fn chatterm_dir() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".into())
        });
        std::path::PathBuf::from(base).join("chatterm")
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(home).join(".chatterm")
    }
}

fn dispatch_pipe_line(app: &AppHandle, line: &str) {
    if line.trim().is_empty() {
        return;
    }
    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
        let sid = msg.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let body = msg.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let msg_cwd = msg.get("cwd").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string());
        if !body.is_empty() {
            let (state, preview) = match msg_type {
                "reply" => (None, Some(body)),
                // Forward body as preview so the Sidebar unread counter bumps
                // even for agents whose Stop hook carries no assistant message
                // (Kiro sometimes lands here; the body is the hook bridge's
                // fallback placeholder in that case).
                "done" => (Some("idle".to_string()), Some(body.clone())),
                "ask" => (Some("asking".to_string()), None),
                "tool" => (Some("thinking".to_string()), None),
                _ => (None, None),
            };
            if state.is_some() || preview.is_some() || msg_cwd.is_some() {
                app.emit("pty-meta", &PtyMeta {
                    session_id: sid, title: None, agent: None,
                    state, preview, notification: None, command: None, cwd: msg_cwd,
                }).ok();
            }
        }
    }
}

/// Unix: FIFO IPC
#[cfg(unix)]
fn start_fifo_listener(app: AppHandle) {
    use std::io::{BufRead, BufReader};

    let dir = chatterm_dir();
    std::fs::create_dir_all(&dir).ok();
    let pipe_path = dir.join("hook.pipe");

    std::fs::remove_file(&pipe_path).ok();
    unsafe {
        libc::mkfifo(
            std::ffi::CString::new(pipe_path.to_str().unwrap()).unwrap().as_ptr(),
            0o622,
        );
    }

    std::thread::spawn(move || loop {
        let file = match std::fs::File::open(&pipe_path) {
            Ok(f) => f,
            Err(_) => { std::thread::sleep(std::time::Duration::from_millis(500)); continue; }
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            dispatch_pipe_line(&app, &line);
        }
    });
}

/// Windows: Named Pipe IPC
#[cfg(windows)]
fn start_fifo_listener(app: AppHandle) {
    use std::io::{BufRead, BufReader};
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::System::Pipes::*;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE;
    use windows::core::PCSTR;

    let dir = chatterm_dir();
    std::fs::create_dir_all(&dir).ok();

    std::thread::spawn(move || {
        let pipe_name = b"\\\\.\\pipe\\chatterm-hook\0";
        // PIPE_ACCESS_INBOUND (0x1) — windows crate exposes this as
        // FILE_FLAGS_AND_ATTRIBUTES which doesn't impl BitOr with the pipe
        // flags, so we define it here until the crate fixes the type.
        const PIPE_ACCESS_INBOUND: windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES =
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0x0000_0001);
        loop {
            let open_mode = PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE;
            let handle = unsafe {
                CreateNamedPipeA(
                    PCSTR(pipe_name.as_ptr()),
                    open_mode,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    1, 4096, 4096, 0, None,
                )
            };
            let handle = match handle {
                Ok(h) => h,
                Err(_) => { std::thread::sleep(std::time::Duration::from_millis(500)); continue; }
            };
            let _ = unsafe { ConnectNamedPipe(handle, None) };
            let file = unsafe { std::fs::File::from_raw_handle(handle.0 as *mut _) };
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                dispatch_pipe_line(&app, &line);
            }
        }
    });
}
