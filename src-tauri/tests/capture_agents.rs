/// Capture real AI CLI PTY output, feed through VScreen, dump screen state.
/// Run: cd src-tauri && cargo test --test capture_agents -- --ignored --nocapture 2>&1 | tee /tmp/agent_capture.txt

use std::io::{Read, Write};
use std::time::{Duration, Instant};
use std::sync::mpsc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use chatterm_lib::vscreen::VScreen;

fn run_agent(name: &str, cmd: &str, inputs: &[(&str, u64)], total_secs: u64) {
    println!("\n======================================================================");
    println!("  CAPTURING: {} (cmd: {})", name, cmd);
    println!("======================================================================\n");

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 }).unwrap();

    let shell = std::env::var("SHELL").unwrap_or("/bin/bash".into());
    let mut command = CommandBuilder::new(&shell);
    command.args(["-l", "-c", cmd]);
    if let Ok(home) = std::env::var("HOME") { command.env("HOME", &home); command.cwd(&home); }
    if let Ok(path) = std::env::var("PATH") {
        let full = if !path.contains("/opt/homebrew/bin") { format!("/opt/homebrew/bin:/usr/local/bin:{path}") } else { path };
        command.env("PATH", full);
    }
    command.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(command).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => { tx.send(buf[..n].to_vec()).ok(); }
            }
        }
    });

    let start = Instant::now();
    let mut vscreen = VScreen::new();
    let mut last_dump = String::new();
    let mut input_idx = 0;

    // Schedule inputs
    let mut next_input_time: Option<Instant> = if !inputs.is_empty() {
        Some(start + Duration::from_secs(inputs[0].1))
    } else { None };

    while start.elapsed() < Duration::from_secs(total_secs) {
        // Send scheduled input
        if let Some(t) = next_input_time {
            if Instant::now() >= t && input_idx < inputs.len() {
                let (text, _) = inputs[input_idx];
                println!("  >>> SENDING INPUT: {:?} at {}s", text, start.elapsed().as_secs());
                writer.write_all(text.as_bytes()).ok();
                writer.flush().ok();
                input_idx += 1;
                next_input_time = if input_idx < inputs.len() {
                    Some(start + Duration::from_secs(inputs[input_idx].1))
                } else { None };
            }
        }

        // Drain PTY output
        std::thread::sleep(Duration::from_millis(200));
        let mut got_data = false;
        while let Ok(chunk) = rx.try_recv() {
            vscreen.feed(&chunk);
            got_data = true;
        }

        if !got_data { continue; }

        // Dump screen if changed
        let rows = vscreen.rows();
        let dump = rows.join("\n");
        if dump != last_dump {
            last_dump = dump;
            let elapsed = start.elapsed().as_millis();
            println!("  --- Screen at {}ms ({} non-empty rows) ---", elapsed, rows.len());
            for (i, row) in rows.iter().enumerate() {
                if row.trim().is_empty() { continue; }
                let trimmed = if row.len() > 110 { format!("{}…", &row[..row.char_indices().nth(107).map(|(i,_)|i).unwrap_or(row.len())]) } else { row.clone() };
                println!("    [{:2}] {}", i, trimmed);
            }
            println!();
        }
    }

    child.kill().ok();
    println!("  DONE: {} ({} seconds)\n", name, start.elapsed().as_secs());
}

#[test]
#[ignore]
fn capture_claude() {
    run_agent("Claude Code", "claude", &[
        ("\n", 5),           // Accept trust prompt
        ("say hi\n", 8),    // Send question
    ], 25);
}

#[test]
#[ignore]
fn capture_kiro() {
    run_agent("Kiro CLI", "kiro-cli chat", &[
        ("say hi\n", 6),    // Send question after init
    ], 25);
}

#[test]
#[ignore]
fn capture_codex() {
    run_agent("Codex", "codex", &[
        ("\n", 5),           // Accept any prompt
        ("say hi\n", 8),    // Send question
    ], 25);
}

#[test]
#[ignore]
fn capture_all() {
    capture_claude();
    capture_kiro();
    capture_codex();
}
