//! PTY driver: spawns the target, tees raw bytes to disk, runs the fixture steps.
//!
//! Two artifact streams are produced per run:
//!   - `raw.bin`  — byte-exact copy of the PTY master read side.
//!   - `cast.json` — asciinema v2 format, replayable via `asciinema play`.
//!   - `stream.ndjson` — per-chunk event log (ts_ms, kind, hex) for analysis.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use regex::bytes::Regex;

use crate::fixture::{unescape, Fixture, Step};

#[allow(dead_code)] // cast_path/stream_path are part of the public artifact API
pub struct RunArtifacts {
    pub dir: PathBuf,
    pub raw_path: PathBuf,
    pub cast_path: PathBuf,
    pub stream_path: PathBuf,
    pub total_bytes: u64,
    pub duration: Duration,
}

pub fn run(fixture: &Fixture, out_root: &Path) -> Result<RunArtifacts, String> {
    let case_dir = out_root.join(&fixture.agent).join(&fixture.case);
    std::fs::create_dir_all(&case_dir)
        .map_err(|e| format!("mkdir {}: {e}", case_dir.display()))?;

    let raw_path = case_dir.join("raw.bin");
    let cast_path = case_dir.join("cast.json");
    let stream_path = case_dir.join("stream.ndjson");

    let mut raw_file = File::create(&raw_path)
        .map_err(|e| format!("create raw.bin: {e}"))?;
    let mut cast_file = File::create(&cast_path)
        .map_err(|e| format!("create cast.json: {e}"))?;
    let mut stream_file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&stream_path)
        .map_err(|e| format!("create stream.ndjson: {e}"))?;

    // asciinema v2 header
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    writeln!(
        cast_file,
        "{}",
        serde_json::json!({
            "version": 2,
            "width": fixture.cols,
            "height": fixture.rows,
            "timestamp": ts,
            "env": { "SHELL": "/bin/bash", "TERM": "xterm-256color" },
            "title": format!("{}/{}", fixture.agent, fixture.case),
        })
    ).map_err(|e| format!("write cast header: {e}"))?;

    // Spawn PTY
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: fixture.rows, cols: fixture.cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("openpty: {e}"))?;

    let argv: Vec<String> = fixture
        .command
        .clone()
        .unwrap_or_else(|| vec![default_shell(), "-l".into()]);
    if argv.is_empty() {
        return Err("fixture.command must be non-empty when set".into());
    }
    let mut cmd = CommandBuilder::new(&argv[0]);
    for a in &argv[1..] {
        cmd.arg(a);
    }
    if let Some(cwd) = &fixture.cwd {
        cmd.cwd(cwd);
    }
    cmd.env("TERM", "xterm-256color");
    // Inherit locale so multibyte output works; mirrors src-tauri/src/pty.rs.
    for var in ["LANG", "LC_ALL", "LC_CTYPE", "HOME", "USER", "PATH"] {
        if let Ok(v) = std::env::var(var) {
            cmd.env(var, v);
        }
    }
    for (k, v) in &fixture.env {
        cmd.env(k, v);
    }

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("spawn: {e}"))?;
    let mut writer = pair.master.take_writer().map_err(|e| format!("take_writer: {e}"))?;
    let reader = pair.master.try_clone_reader().map_err(|e| format!("clone_reader: {e}"))?;

    let (tx, rx): (Sender<Chunk>, Receiver<Chunk>) = channel();
    let started = Instant::now();
    // Accumulated raw bytes shared with the main thread for regex matching.
    let accum = Arc::new(Mutex::new(Vec::<u8>::new()));

    // Reader thread: byte-exact copy to disk + forward to main thread.
    let tx_reader = tx.clone();
    let accum_reader = accum.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut reader = reader;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    let ts = started.elapsed();
                    accum_reader.lock().unwrap().extend_from_slice(&data);
                    if tx_reader.send(Chunk { ts, data }).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Main loop: drain reader events, execute steps.
    let mut total_bytes: u64 = 0;
    let mut cursor = 0usize; // search position into accum
    let deadline = Instant::now() + Duration::from_millis(fixture.timeout_ms);

    // Pre-step: drain initial banner for up to 200ms before first step,
    // so fixtures don't race with shell prompt emission.
    drain_quiet(
        &rx,
        &mut raw_file,
        &mut cast_file,
        &mut stream_file,
        started,
        &mut total_bytes,
        Duration::from_millis(200),
    )?;

    'steps: for (i, step) in fixture.step.iter().enumerate() {
        match step {
            Step::Send { data } => {
                let bytes = unescape(data);
                writer.write_all(&bytes).map_err(|e| format!("step {i} write: {e}"))?;
                writer.flush().ok();
                log_event(&mut stream_file, started.elapsed(), "write", &bytes)?;
            }
            Step::WaitIdle { ms } => {
                let idle = Duration::from_millis(ms.unwrap_or(fixture.idle_ms_default));
                wait_idle(&rx, &mut raw_file, &mut cast_file, &mut stream_file, started, &mut total_bytes, idle, deadline)?;
            }
            Step::WaitRegex { pattern, timeout_ms } => {
                // Force byte-semantics: without (?-u) the regex crate treats
                // `\xNN` as a Unicode code point, so `\xe2` would try to match
                // UTF-8 `c3 a2` instead of the raw byte 0xE2 we actually want.
                let full = format!("(?-u){pattern}");
                let re = Regex::new(&full).map_err(|e| format!("bad regex {pattern:?}: {e}"))?;
                let step_deadline = Instant::now() + Duration::from_millis(*timeout_ms);
                loop {
                    // Check existing accumulator first (don't miss hits that arrived during prior step).
                    {
                        let buf = accum.lock().unwrap();
                        if let Some(m) = re.find_at(&buf, cursor) {
                            cursor = m.end();
                            break;
                        }
                    }
                    if Instant::now() > step_deadline || Instant::now() > deadline {
                        return Err(format!(
                            "step {i} wait_regex {pattern:?}: timeout after {timeout_ms}ms"
                        ));
                    }
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(chunk) => {
                            total_bytes += chunk.data.len() as u64;
                            raw_file.write_all(&chunk.data).map_err(|e| format!("raw write: {e}"))?;
                            write_cast(&mut cast_file, chunk.ts, &chunk.data)?;
                            log_event(&mut stream_file, chunk.ts, "read", &chunk.data)?;
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break 'steps,
                    }
                }
            }
            Step::Resize { cols, rows } => {
                pair.master
                    .resize(PtySize { rows: *rows, cols: *cols, pixel_width: 0, pixel_height: 0 })
                    .map_err(|e| format!("resize: {e}"))?;
                log_event(
                    &mut stream_file,
                    started.elapsed(),
                    "resize",
                    format!("{cols}x{rows}").as_bytes(),
                )?;
            }
            Step::Sleep { ms } => {
                drain_quiet(
                    &rx,
                    &mut raw_file,
                    &mut cast_file,
                    &mut stream_file,
                    started,
                    &mut total_bytes,
                    Duration::from_millis(*ms),
                )?;
            }
        }
    }

    // Post-steps: let the process wind down for a beat, then kill and drain.
    drain_quiet(
        &rx,
        &mut raw_file,
        &mut cast_file,
        &mut stream_file,
        started,
        &mut total_bytes,
        Duration::from_millis(300),
    )?;
    let _ = child.kill();
    drain_quiet(
        &rx,
        &mut raw_file,
        &mut cast_file,
        &mut stream_file,
        started,
        &mut total_bytes,
        Duration::from_millis(200),
    )?;

    Ok(RunArtifacts {
        dir: case_dir,
        raw_path,
        cast_path,
        stream_path,
        total_bytes,
        duration: started.elapsed(),
    })
}

struct Chunk {
    ts: Duration,
    data: Vec<u8>,
}

fn wait_idle(
    rx: &Receiver<Chunk>,
    raw: &mut File,
    cast: &mut File,
    stream: &mut File,
    _started: Instant,
    total: &mut u64,
    idle: Duration,
    hard_deadline: Instant,
) -> Result<(), String> {
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        if now >= hard_deadline {
            return Ok(());
        }
        let remaining = idle.saturating_sub(now.duration_since(last));
        if remaining.is_zero() {
            return Ok(());
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(chunk) => {
                *total += chunk.data.len() as u64;
                raw.write_all(&chunk.data).map_err(|e| format!("raw write: {e}"))?;
                write_cast(cast, chunk.ts, &chunk.data)?;
                log_event(stream, chunk.ts, "read", &chunk.data)?;
                last = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                if last.elapsed() >= idle {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn drain_quiet(
    rx: &Receiver<Chunk>,
    raw: &mut File,
    cast: &mut File,
    stream: &mut File,
    _started: Instant,
    total: &mut u64,
    max: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + max;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(chunk) => {
                *total += chunk.data.len() as u64;
                raw.write_all(&chunk.data).map_err(|e| format!("raw write: {e}"))?;
                write_cast(cast, chunk.ts, &chunk.data)?;
                log_event(stream, chunk.ts, "read", &chunk.data)?;
            }
            Err(RecvTimeoutError::Timeout) => return Ok(()),
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn write_cast(cast: &mut File, ts: Duration, data: &[u8]) -> Result<(), String> {
    // asciinema v2 wants UTF-8 strings. Use lossy conversion so byte-exact
    // stays in raw.bin while cast.json stays replayable.
    let s = String::from_utf8_lossy(data);
    let line = serde_json::json!([ts.as_secs_f64(), "o", s]);
    writeln!(cast, "{}", line).map_err(|e| format!("cast write: {e}"))
}

fn log_event(stream: &mut File, ts: Duration, kind: &str, data: &[u8]) -> Result<(), String> {
    let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
    let line = serde_json::json!({
        "ts_ms": ts.as_millis() as u64,
        "kind": kind,
        "len": data.len(),
        "hex": hex,
    });
    writeln!(stream, "{}", line).map_err(|e| format!("stream write: {e}"))
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
}
