//! `capture` — headless PTY fixture runner + ANSI coverage analyzer.
//!
//! Usage:
//!   capture run   <fixture.toml> [--out <dir>]
//!   capture analyze <raw.bin> [--out <coverage.json>]
//!   capture coverage <fixtures-dir> [--out <dir>]   (run all + union coverage)

mod analyzer;
mod fixture;
mod recorder;
mod report;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn usage() -> &'static str {
    "usage:\n  \
     capture run <fixture.toml> [--out <dir>]\n  \
     capture analyze <raw.bin> [--out <coverage.json>]\n  \
     capture coverage <fixtures-dir> [--out <dir>]\n  \
     capture report <artifacts-dir> [--out <report.html>]\n"
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprint!("{}", usage());
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "run" => cmd_run(&args[1..]),
        "analyze" => cmd_analyze(&args[1..]),
        "coverage" => cmd_coverage(&args[1..]),
        "report" => cmd_report(&args[1..]),
        "-h" | "--help" => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            eprint!("{}", usage());
            ExitCode::from(2)
        }
    }
}

fn parse_out(args: &[String]) -> (Option<&str>, Option<&str>) {
    let mut positional: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--out" && i + 1 < args.len() {
            out = Some(&args[i + 1]);
            i += 2;
        } else if positional.is_none() {
            positional = Some(&args[i]);
            i += 1;
        } else {
            i += 1;
        }
    }
    (positional, out)
}

fn cmd_run(args: &[String]) -> ExitCode {
    let (Some(path), out) = parse_out(args) else {
        eprintln!("run: missing fixture path");
        return ExitCode::from(2);
    };
    let out_root = PathBuf::from(out.unwrap_or("artifacts"));

    let fix = match fixture::load(Path::new(path)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("load fixture: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("==> running {}/{}", fix.agent, fix.case);
    let result = match recorder::run(&fix, &out_root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("run failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "    captured {} bytes in {:?} -> {}",
        result.total_bytes,
        result.duration,
        result.dir.display()
    );

    // Auto-analyze so the user sees coverage immediately.
    match analyzer::analyze(&result.raw_path) {
        Ok(cov) => {
            let cov_path = result.dir.join("coverage.json");
            if let Ok(json) = serde_json::to_string_pretty(&cov) {
                std::fs::write(&cov_path, json).ok();
            }
            println!("{}", analyzer::render_summary(&cov));
        }
        Err(e) => eprintln!("analyze: {e}"),
    }
    ExitCode::SUCCESS
}

fn cmd_analyze(args: &[String]) -> ExitCode {
    let (Some(path), out) = parse_out(args) else {
        eprintln!("analyze: missing raw.bin path");
        return ExitCode::from(2);
    };
    match analyzer::analyze(Path::new(path)) {
        Ok(cov) => {
            let json = serde_json::to_string_pretty(&cov).unwrap();
            if let Some(p) = out {
                if let Err(e) = std::fs::write(p, &json) {
                    eprintln!("write {p}: {e}");
                    return ExitCode::FAILURE;
                }
                eprintln!("wrote {p}");
            } else {
                println!("{}", json);
            }
            println!("{}", analyzer::render_summary(&cov));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("analyze: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_coverage(args: &[String]) -> ExitCode {
    let (Some(dir), out) = parse_out(args) else {
        eprintln!("coverage: missing fixtures dir");
        return ExitCode::from(2);
    };
    let out_root = PathBuf::from(out.unwrap_or("artifacts"));

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("read_dir {dir}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut fixtures: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|s| s == "toml").unwrap_or(false))
        .collect();
    fixtures.sort();

    let mut matrix: Vec<(String, analyzer::Coverage)> = Vec::new();
    for f in &fixtures {
        let fix = match fixture::load(f) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip {}: {e}", f.display());
                continue;
            }
        };
        eprintln!("==> {}/{}", fix.agent, fix.case);
        match recorder::run(&fix, &out_root) {
            Ok(result) => match analyzer::analyze(&result.raw_path) {
                Ok(cov) => {
                    let cov_path = result.dir.join("coverage.json");
                    if let Ok(json) = serde_json::to_string_pretty(&cov) {
                        std::fs::write(&cov_path, json).ok();
                    }
                    matrix.push((format!("{}/{}", fix.agent, fix.case), cov));
                }
                Err(e) => eprintln!("analyze {}: {e}", result.raw_path.display()),
            },
            Err(e) => eprintln!("run {}: {e}", f.display()),
        }
    }

    if matrix.is_empty() {
        eprintln!("no successful fixture runs");
        return ExitCode::FAILURE;
    }
    print_matrix(&matrix);
    ExitCode::SUCCESS
}

fn print_matrix(matrix: &[(String, analyzer::Coverage)]) {
    println!("\n=== coverage matrix ===");
    // Header
    print!("{:<36}", "category");
    for (name, _) in matrix {
        print!(" {:>14}", truncate(name, 14));
    }
    println!();
    for cat in analyzer::KNOWN_CATEGORIES {
        print!("{:<36}", cat);
        for (_, cov) in matrix {
            let n = cov.categories.get(*cat).map(|b| b.count).unwrap_or(0);
            if n == 0 {
                print!(" {:>14}", "·");
            } else {
                print!(" {:>14}", n);
            }
        }
        println!();
    }
    // Union gap report
    let mut gaps: Vec<&str> = Vec::new();
    for cat in analyzer::KNOWN_CATEGORIES {
        if matrix.iter().all(|(_, c)| !c.categories.contains_key(*cat)) {
            gaps.push(cat);
        }
    }
    if !gaps.is_empty() {
        println!("\nuncovered across all fixtures: {}", gaps.join(", "));
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n { s.into() } else { format!("…{}", &s[s.len() - n + 1..]) }
}

fn cmd_report(args: &[String]) -> ExitCode {
    let (Some(dir), out) = parse_out(args) else {
        eprintln!("report: missing artifacts dir");
        return ExitCode::from(2);
    };
    let reports = match report::load_all(Path::new(dir)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("load: {e}");
            return ExitCode::FAILURE;
        }
    };
    if reports.is_empty() {
        eprintln!("no coverage.json files found under {dir}. Run `capture coverage` first.");
        return ExitCode::FAILURE;
    }
    let html = report::render(&reports);
    let out_path = out.unwrap_or("report.html");
    if let Err(e) = std::fs::write(out_path, html) {
        eprintln!("write {out_path}: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!("wrote {out_path} ({} case(s))", reports.len());
    ExitCode::SUCCESS
}
