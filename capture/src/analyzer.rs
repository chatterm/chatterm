//! ANSI coverage analyzer.
//!
//! Feeds `raw.bin` through a VTE parser and buckets every event into one of
//! ~15 categories. Emits `coverage.json`:
//!
//! { "total_bytes": N, "categories": {...}, "decset_modes": {...},
//!   "osc_codes": {...}, "sgr_attrs": {...}, "unknown": [...] }

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vte::{Params, Parser, Perform};

#[derive(Default, Serialize, Deserialize)]
pub struct Coverage {
    pub total_bytes: u64,
    pub categories: BTreeMap<String, Bucket>,
    pub decset_modes: BTreeMap<u16, u64>,
    pub osc_codes: BTreeMap<String, u64>,
    pub sgr_attrs: BTreeMap<u16, u64>,
    pub c0_controls: BTreeMap<String, u64>,
    pub unknown: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Bucket {
    pub count: u64,
    pub first_offset: Option<u64>,
    pub examples: Vec<String>,
}

impl Bucket {
    fn hit(&mut self, offset: u64, sample: String) {
        self.count += 1;
        if self.first_offset.is_none() {
            self.first_offset = Some(offset);
        }
        if self.examples.len() < 3 && !self.examples.contains(&sample) {
            self.examples.push(sample);
        }
    }
}

pub fn analyze(raw: &Path) -> Result<Coverage, String> {
    let bytes = fs::read(raw).map_err(|e| format!("read {}: {e}", raw.display()))?;
    let mut perf = CoveragePerform {
        cov: Coverage {
            total_bytes: bytes.len() as u64,
            ..Default::default()
        },
        offset: 0,
    };
    let mut parser = Parser::new();
    // Feed one byte at a time so `perf.offset` stays accurate for first-seen
    // tracking. vte 0.15 wants a slice, so wrap each byte in a 1-element array.
    for (i, b) in bytes.iter().enumerate() {
        perf.offset = i as u64;
        parser.advance(&mut perf, &[*b]);
    }
    Ok(perf.cov)
}

struct CoveragePerform {
    cov: Coverage,
    offset: u64,
}

impl CoveragePerform {
    fn bump(&mut self, key: &str, sample: impl Into<String>) {
        let off = self.offset;
        self.cov
            .categories
            .entry(key.to_string())
            .or_default()
            .hit(off, sample.into());
    }
}

/// Categories tracked (at minimum — more are inserted on first hit):
pub const KNOWN_CATEGORIES: &[&str] = &[
    "print",
    "c0",
    "csi_sgr",
    "csi_cursor",
    "csi_erase",
    "csi_edit",
    "csi_mode_set",
    "csi_mode_reset",
    "csi_scroll",
    "csi_report",
    "csi_other",
    "osc_title",
    "osc_hyperlink",
    "osc_clipboard",
    "osc_shell_integration",
    "osc_color",
    "osc_cwd",
    "osc_other",
    "dcs",
    "esc_other",
];

fn params_to_string(params: &Params) -> String {
    let mut parts = Vec::new();
    for p in params.iter() {
        let sub: Vec<String> = p.iter().map(|n| n.to_string()).collect();
        parts.push(sub.join(":"));
    }
    parts.join(";")
}

fn escape_for_display(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            0x1b => s.push_str("\\e"),
            0x07 => s.push_str("\\a"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\x{b:02x}")),
        }
    }
    s
}

impl Perform for CoveragePerform {
    fn print(&mut self, _c: char) {
        self.cov
            .categories
            .entry("print".into())
            .or_default()
            .count += 1;
    }

    fn execute(&mut self, byte: u8) {
        let name = match byte {
            0x07 => "BEL",
            0x08 => "BS",
            0x09 => "HT",
            0x0a => "LF",
            0x0b => "VT",
            0x0c => "FF",
            0x0d => "CR",
            0x0e => "SO",
            0x0f => "SI",
            _ => "C0_OTHER",
        };
        *self.cov.c0_controls.entry(name.into()).or_insert(0) += 1;
        let off = self.offset;
        self.cov
            .categories
            .entry("c0".into())
            .or_default()
            .hit(off, format!("\\x{byte:02x} ({name})"));
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        let private = intermediates.first().copied() == Some(b'?');
        let p_str = params_to_string(params);
        let sample = format!(
            "\\e[{}{}{}",
            if private { "?" } else { "" },
            p_str,
            action
        );

        match action {
            'm' => {
                self.bump("csi_sgr", &sample);
                for p in params.iter() {
                    if let Some(&n) = p.first() {
                        *self.cov.sgr_attrs.entry(n).or_insert(0) += 1;
                    }
                }
            }
            'A' | 'B' | 'C' | 'D' | 'E' | 'F' | 'G' | 'H' | 'f' | 'd' | 's' | 'u' => {
                self.bump("csi_cursor", &sample);
            }
            // ED / EL / ECH — true erasures that clear characters in place.
            'J' | 'K' | 'X' => self.bump("csi_erase", &sample),
            // ICH / IL / DL / DCH — line/char edits that shift content.
            '@' | 'L' | 'M' | 'P' => self.bump("csi_edit", &sample),
            'h' | 'l' => {
                let cat = if action == 'h' { "csi_mode_set" } else { "csi_mode_reset" };
                self.bump(cat, &sample);
                if private {
                    for p in params.iter() {
                        if let Some(&n) = p.first() {
                            *self.cov.decset_modes.entry(n).or_insert(0) += 1;
                        }
                    }
                }
            }
            'r' | 'S' | 'T' => self.bump("csi_scroll", &sample),
            'n' | 'c' => self.bump("csi_report", &sample),
            _ => self.bump("csi_other", &sample),
        }

        if ignore {
            self.cov
                .unknown
                .push(format!("ignored csi {sample}"));
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let first = params.first().copied().unwrap_or(&[]);
        let code = std::str::from_utf8(first).unwrap_or("").to_string();
        let bucket = match code.as_str() {
            "0" | "1" | "2" => "osc_title",
            "4" | "10" | "11" | "12" | "17" | "19" | "104" | "110" | "111" => "osc_color",
            "7" => "osc_cwd",
            "8" => "osc_hyperlink",
            "52" => "osc_clipboard",
            "133" | "633" | "697" => "osc_shell_integration",
            _ => "osc_other",
        };
        *self.cov.osc_codes.entry(code.clone()).or_insert(0) += 1;

        // Render a compact sample: only the code + truncated payload.
        let payload: Vec<u8> = params
            .iter()
            .skip(1)
            .flat_map(|p| {
                let mut v = p.to_vec();
                v.push(b';');
                v
            })
            .collect();
        let payload_str = escape_for_display(&payload);
        let truncated = if payload_str.len() > 40 {
            format!("{}...", &payload_str[..40])
        } else {
            payload_str
        };
        let sample = format!("\\e]{code};{truncated}\\a");
        self.bump(bucket, sample);
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let sample = format!(
            "\\eP{}{}{}",
            escape_for_display(intermediates),
            params_to_string(params),
            action
        );
        self.bump("dcs", sample);
    }

    fn put(&mut self, _byte: u8) {
        // DCS payload byte — counted implicitly via hook().
    }

    fn unhook(&mut self) {}

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        let sample = format!(
            "\\e{}{}",
            escape_for_display(intermediates),
            byte as char
        );
        self.bump("esc_other", sample);
    }
}

/// Human-readable name for a DEC private mode number (CSI `?<n>h/l`).
/// Shared by the CLI summary, the HTML report, and anyone else who wants a
/// consistent label. Unknown modes return an empty string.
pub fn decset_name(m: u16) -> &'static str {
    match m {
        1 => "app-cursor-keys",
        7 => "auto-wrap",
        25 => "cursor-visibility",
        47 => "alt-screen (legacy)",
        1000 => "mouse x10",
        1002 => "mouse button+drag",
        1003 => "mouse any-motion",
        1004 => "focus in/out",
        1006 => "mouse SGR",
        1034 => "eightBitInput",
        1049 => "alt-screen (save+clear)",
        2004 => "bracketed paste",
        // Kitty/WezTerm: terminal notifies app when OS color scheme changes
        // (light/dark toggle). Seen in the wild from Claude Code.
        2031 => "color-scheme notify",
        _ => "",
    }
}

/// Summary formatted for the terminal.
pub fn render_summary(cov: &Coverage) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "bytes={} categories={}\n",
        cov.total_bytes,
        cov.categories.len()
    ));

    // Category hit/miss table vs KNOWN_CATEGORIES so gaps are obvious.
    out.push_str("ANSI coverage:\n");
    for cat in KNOWN_CATEGORIES {
        if let Some(b) = cov.categories.get(*cat) {
            out.push_str(&format!("  [x] {:<24} {:>5}  e.g. {}\n",
                cat, b.count, b.examples.first().map(|s| s.as_str()).unwrap_or("")));
        } else {
            out.push_str(&format!("  [ ] {:<24}     0\n", cat));
        }
    }
    // Report anything we saw but didn't pre-declare.
    for (k, b) in &cov.categories {
        if !KNOWN_CATEGORIES.contains(&k.as_str()) {
            out.push_str(&format!("  [?] {:<24} {:>5}  (unclassified)\n", k, b.count));
        }
    }

    if !cov.decset_modes.is_empty() {
        out.push_str("DECSET/RST modes:\n");
        for (m, n) in &cov.decset_modes {
            let label = decset_name(*m);
            let label = if label.is_empty() { "?" } else { label };
            out.push_str(&format!("  ?{m:<5} {n:>5}  {label}\n"));
        }
    }
    if !cov.osc_codes.is_empty() {
        out.push_str("OSC codes:\n");
        for (c, n) in &cov.osc_codes {
            out.push_str(&format!("  {c:<6} {n:>5}\n"));
        }
    }
    out
}
