//! Self-contained HTML report.
//!
//! Walks `<artifacts-dir>/<agent>/<case>/coverage.json`, renders a single
//! static HTML file with a coverage heatmap and per-case drill-downs.
//! No external JS/CSS — the file works offline.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::analyzer::{decset_name, Coverage, KNOWN_CATEGORIES};

pub struct CaseReport {
    pub agent: String,
    pub case: String,
    pub dir: PathBuf,
    pub coverage: Coverage,
}

pub fn load_all(root: &Path) -> Result<Vec<CaseReport>, String> {
    let mut out = Vec::new();
    let agents = fs::read_dir(root).map_err(|e| format!("read_dir {}: {e}", root.display()))?;
    for a in agents.flatten() {
        if !a.path().is_dir() {
            continue;
        }
        let agent = a.file_name().to_string_lossy().to_string();
        let cases = match fs::read_dir(a.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for c in cases.flatten() {
            let cpath = c.path();
            if !cpath.is_dir() {
                continue;
            }
            let cov_path = cpath.join("coverage.json");
            if !cov_path.exists() {
                continue;
            }
            let text = fs::read_to_string(&cov_path)
                .map_err(|e| format!("read {}: {e}", cov_path.display()))?;
            let coverage: Coverage = serde_json::from_str(&text)
                .map_err(|e| format!("parse {}: {e}", cov_path.display()))?;
            out.push(CaseReport {
                agent: agent.clone(),
                case: c.file_name().to_string_lossy().to_string(),
                dir: cpath,
                coverage,
            });
        }
    }
    out.sort_by(|a, b| (a.agent.clone(), a.case.clone()).cmp(&(b.agent.clone(), b.case.clone())));
    Ok(out)
}

pub fn render(reports: &[CaseReport]) -> String {
    let mut html = String::new();
    html.push_str(HEAD);
    html.push_str("<h1>ChatTerm ANSI Coverage</h1>");

    // Summary
    html.push_str(&format!(
        "<p class=meta>{} case(s) loaded. Replay any recording with \
         <code>asciinema play &lt;artifact-dir&gt;/cast.json</code>.</p>",
        reports.len()
    ));

    // Matrix heatmap
    html.push_str("<h2>Coverage matrix</h2>");
    html.push_str("<table class=matrix><thead><tr><th>category</th>");
    for r in reports {
        html.push_str(&format!(
            "<th><a href=\"#{a}-{c}\">{a}/{c}</a></th>",
            a = esc(&r.agent),
            c = esc(&r.case)
        ));
    }
    html.push_str("</tr></thead><tbody>");
    // Find per-category max across reports for heatmap scaling.
    let mut per_cat_max: BTreeMap<&str, u64> = BTreeMap::new();
    for cat in KNOWN_CATEGORIES {
        let m = reports
            .iter()
            .map(|r| r.coverage.categories.get(*cat).map(|b| b.count).unwrap_or(0))
            .max()
            .unwrap_or(0);
        per_cat_max.insert(*cat, m);
    }
    for cat in KNOWN_CATEGORIES {
        html.push_str(&format!("<tr><td class=cat>{}</td>", cat));
        let max = *per_cat_max.get(cat).unwrap_or(&0);
        for r in reports {
            let n = r.coverage.categories.get(*cat).map(|b| b.count).unwrap_or(0);
            let (cls, text) = if n == 0 {
                ("zero", "·".to_string())
            } else {
                (heat_class(n, max), n.to_string())
            };
            html.push_str(&format!("<td class=\"cell {}\">{}</td>", cls, text));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");

    // Gap report
    let gaps: Vec<&str> = KNOWN_CATEGORIES
        .iter()
        .copied()
        .filter(|cat| {
            reports
                .iter()
                .all(|r| !r.coverage.categories.contains_key(*cat))
        })
        .collect();
    if !gaps.is_empty() {
        html.push_str(&format!(
            "<p class=gap><strong>Uncovered across all cases:</strong> {}</p>",
            esc(&gaps.join(", "))
        ));
    }

    // Per-case details
    html.push_str("<h2>Case details</h2>");
    for r in reports {
        html.push_str(&format!(
            "<section class=case id=\"{a}-{c}\"><h3>{a} / {c}</h3>",
            a = esc(&r.agent),
            c = esc(&r.case)
        ));
        html.push_str(&format!(
            "<p class=meta>bytes: {} · categories hit: {} · artifacts: <code>{}</code></p>",
            r.coverage.total_bytes,
            r.coverage.categories.len(),
            esc(&r.dir.display().to_string())
        ));

        html.push_str("<table class=detail><tr><th>category</th><th>count</th><th>first offset</th><th>examples</th></tr>");
        for cat in KNOWN_CATEGORIES {
            let Some(b) = r.coverage.categories.get(*cat) else { continue };
            let examples = b
                .examples
                .iter()
                .map(|e| format!("<code>{}</code>", esc(e)))
                .collect::<Vec<_>>()
                .join(" ");
            html.push_str(&format!(
                "<tr><td>{}</td><td class=num>{}</td><td class=num>{}</td><td>{}</td></tr>",
                cat,
                b.count,
                b.first_offset.map(|o| o.to_string()).unwrap_or_default(),
                examples
            ));
        }
        // Unclassified
        for (cat, b) in &r.coverage.categories {
            if !KNOWN_CATEGORIES.contains(&cat.as_str()) {
                let examples = b
                    .examples
                    .iter()
                    .map(|e| format!("<code>{}</code>", esc(e)))
                    .collect::<Vec<_>>()
                    .join(" ");
                html.push_str(&format!(
                    "<tr class=unk><td>? {}</td><td class=num>{}</td><td class=num>{}</td><td>{}</td></tr>",
                    cat,
                    b.count,
                    b.first_offset.map(|o| o.to_string()).unwrap_or_default(),
                    examples
                ));
            }
        }
        html.push_str("</table>");

        if !r.coverage.decset_modes.is_empty() {
            html.push_str("<h4>DECSET / DECRST modes</h4><table class=detail><tr><th>mode</th><th>count</th><th>name</th></tr>");
            for (m, n) in &r.coverage.decset_modes {
                html.push_str(&format!(
                    "<tr><td>?{}</td><td class=num>{}</td><td>{}</td></tr>",
                    m,
                    n,
                    decset_name(*m)
                ));
            }
            html.push_str("</table>");
        }
        if !r.coverage.osc_codes.is_empty() {
            html.push_str("<h4>OSC codes</h4><table class=detail><tr><th>code</th><th>count</th></tr>");
            for (c, n) in &r.coverage.osc_codes {
                html.push_str(&format!("<tr><td>{}</td><td class=num>{}</td></tr>", esc(c), n));
            }
            html.push_str("</table>");
        }
        if !r.coverage.sgr_attrs.is_empty() {
            html.push_str("<h4>SGR attributes</h4><table class=detail><tr><th>attr</th><th>count</th></tr>");
            for (a, n) in &r.coverage.sgr_attrs {
                html.push_str(&format!(
                    "<tr><td>{} <span class=muted>{}</span></td><td class=num>{}</td></tr>",
                    a,
                    sgr_name(*a),
                    n
                ));
            }
            html.push_str("</table>");
        }

        html.push_str("</section>");
    }

    html.push_str("</body></html>");
    html
}

fn heat_class(n: u64, max: u64) -> &'static str {
    if max == 0 {
        return "zero";
    }
    let ratio = n as f64 / max as f64;
    if ratio >= 0.66 { "hot" } else if ratio >= 0.33 { "warm" } else { "cool" }
}

fn sgr_name(a: u16) -> &'static str {
    match a {
        0 => "reset",
        1 => "bold",
        2 => "dim",
        3 => "italic",
        4 => "underline",
        5 => "blink",
        7 => "reverse",
        8 => "conceal",
        9 => "strike",
        22 => "normal-weight",
        23 => "no-italic",
        24 => "no-underline",
        27 => "no-reverse",
        29 => "no-strike",
        30..=37 => "fg basic",
        38 => "fg extended",
        39 => "fg default",
        40..=47 => "bg basic",
        48 => "bg extended",
        49 => "bg default",
        90..=97 => "fg bright",
        100..=107 => "bg bright",
        _ => "",
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const HEAD: &str = r#"<!doctype html>
<html><head><meta charset=utf-8><title>ChatTerm ANSI Coverage</title>
<style>
:root { color-scheme: light dark; }
body { font: 14px/1.45 -apple-system, system-ui, sans-serif; max-width: 1200px; margin: 2em auto; padding: 0 1em; }
h1 { margin-bottom: 0.2em; }
.meta { color: #777; font-size: 13px; }
code { background: rgba(127,127,127,0.12); padding: 1px 4px; border-radius: 3px; font-size: 12px; }
table { border-collapse: collapse; margin: 0.4em 0 1.2em; }
th, td { padding: 4px 10px; text-align: left; border: 1px solid rgba(127,127,127,0.2); vertical-align: top; }
.matrix th:first-child, .matrix td.cat { text-align: left; font-family: ui-monospace, Menlo, monospace; font-size: 12px; }
.matrix th { writing-mode: horizontal-tb; font-weight: 500; font-size: 12px; }
.matrix .cell { text-align: right; font-variant-numeric: tabular-nums; font-family: ui-monospace, Menlo, monospace; }
.matrix .zero { color: #aaa; }
.matrix .cool { background: rgba(60, 160, 220, 0.18); }
.matrix .warm { background: rgba(240, 180, 60, 0.25); }
.matrix .hot  { background: rgba(230, 100, 60, 0.30); }
.gap { background: rgba(255, 200, 60, 0.15); padding: 8px 12px; border-left: 3px solid orange; }
.case { margin-top: 1.5em; padding-top: 0.6em; border-top: 1px solid rgba(127,127,127,0.2); }
.detail .num { text-align: right; font-variant-numeric: tabular-nums; }
.detail td:first-child { font-family: ui-monospace, Menlo, monospace; font-size: 12px; }
.detail tr.unk { background: rgba(255,200,60,0.12); }
.muted { color: #888; font-size: 11px; margin-left: 4px; }
h3 a { text-decoration: none; }
</style></head><body>
"#;
