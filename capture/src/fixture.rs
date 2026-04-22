//! Fixture schema: TOML-described script for driving an agent session.
//!
//! A fixture declares *what* to spawn, *what* to type, *how long* to wait,
//! and *what* patterns must appear. The recorder executes it step-by-step
//! and dumps raw bytes for later analysis.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub agent: String,
    pub case: String,
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_idle_ms")]
    pub idle_ms_default: u64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub step: Vec<Step>,
}

fn default_cols() -> u16 { 120 }
fn default_rows() -> u16 { 30 }
fn default_idle_ms() -> u64 { 1500 }
fn default_timeout_ms() -> u64 { 60_000 }

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    /// Write bytes to PTY. `data` supports \xNN, \r, \n, \t, \e (= ESC) escapes.
    Send { data: String },
    /// Block until PTY has been silent for `ms` (or `idle_ms_default`).
    WaitIdle {
        #[serde(default)]
        ms: Option<u64>,
    },
    /// Block until regex matches in accumulated raw output. Fails on timeout.
    WaitRegex {
        pattern: String,
        #[serde(default = "default_wait_timeout")]
        timeout_ms: u64,
    },
    /// Resize PTY window.
    Resize { cols: u16, rows: u16 },
    /// Sleep unconditionally.
    Sleep { ms: u64 },
}

fn default_wait_timeout() -> u64 { 10_000 }

/// Convert the TOML-decoded `send.data` string into raw bytes for the PTY.
///
/// We deliberately do *not* define our own escape syntax — TOML's own string
/// escapes (`\n`, `\r`, `\t`, `\u00XX`) already cover the cases we need, and
/// a second layer made it dangerously easy to emit a real ESC byte where the
/// shell expected the literal two-character sequence `\e` for `printf` to
/// interpret. If you need a control byte directly, use TOML `""`.
/// Expand a small set of unambiguous `<NAME>` key tokens to raw bytes.
/// Rationale: TOML basic strings forbid literal `\x00..\x1F` (except tab), so
/// authoring fixtures that need ESC / Ctrl-C / Ctrl-D requires escape gymnastics
/// (`` etc). These angle-bracket tokens never appear in naturally-typed
/// shell or agent input, so we can safely substitute them here without polluting
/// the shell-printf workflow (`"\\e[31m"` still reaches the PTY as 5 literal chars).
pub fn unescape(s: &str) -> Vec<u8> {
    const SUBS: &[(&str, &[u8])] = &[
        ("<ESC>", &[0x1B]),
        ("<C-c>", &[0x03]),
        ("<C-d>", &[0x04]),
        ("<C-C>", &[0x03]),
        ("<C-D>", &[0x04]),
        ("<TAB>", &[0x09]),
        ("<BS>", &[0x08]),
        ("<CR>", &[0x0D]),
        ("<LF>", &[0x0A]),
    ];
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    'outer: while i < bytes.len() {
        if bytes[i] == b'<' {
            for (token, repl) in SUBS {
                let t = token.as_bytes();
                if i + t.len() <= bytes.len() && &bytes[i..i + t.len()] == t {
                    out.extend_from_slice(repl);
                    i += t.len();
                    continue 'outer;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

pub fn load(path: &std::path::Path) -> Result<Fixture, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read fixture {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("parse fixture: {e}"))
}

#[cfg(test)]
mod tests {
    use super::unescape;

    #[test]
    fn passes_through_bytes() {
        // TOML-level escapes are already resolved by the TOML parser; the
        // fixture string reaches us verbatim. Shell-printf workflow preserved.
        assert_eq!(unescape("\\e[31m"), b"\\e[31m".to_vec());
        assert_eq!(unescape("\r\n"), b"\r\n".to_vec());
    }

    #[test]
    fn expands_key_tokens() {
        assert_eq!(unescape("<ESC>"), vec![0x1B]);
        assert_eq!(unescape("<C-c>"), vec![0x03]);
        assert_eq!(unescape("<C-d>"), vec![0x04]);
        assert_eq!(unescape("/exit<CR>"), b"/exit\r".to_vec());
        assert_eq!(unescape("hi<ESC>[13u"), vec![b'h', b'i', 0x1B, b'[', b'1', b'3', b'u']);
    }

    #[test]
    fn unknown_angle_brackets_pass_through() {
        // Things that look like tokens but aren't should survive unchanged.
        assert_eq!(unescape("<foo>"), b"<foo>".to_vec());
        assert_eq!(unescape("<html>"), b"<html>".to_vec());
    }
}
