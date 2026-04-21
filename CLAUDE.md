# ChatTerm

IM-style terminal workspace for AI coding agents. Tauri 2 + React/TS + Rust/portable-pty.

Start with [README.md](README.md) for what the app does, install methods, and hook setup.
Everything below is project-specific context not repeated in the README.

## File index

| Look here for | File |
|---|---|
| Tauri commands + FIFO listener (reads `~/.chatterm/hook.pipe`, emits `pty-meta`) | `src-tauri/src/lib.rs` |
| PTY manager, agent detection, cwd/command probing, platform helpers | `src-tauri/src/pty.rs` |
| Virtual screen buffer (VTE-parsed for preview extraction and chrome stripping) | `src-tauri/src/vscreen.rs` |
| Agent patterns (OSC titles, thinking regex, chrome filters) | `src-tauri/agents.json` + `src-tauri/src/agent_config.rs` |
| Theme parsing incl. macOS Terminal `.terminal` import | `src-tauri/src/theme.rs` |
| Session metadata persistence | `src-tauri/src/session.rs` |
| App state, PTY event routing, shortcut handlers | `src/App.tsx` |
| Hook installer (writes `~/.chatterm/hook.sh`, patches agent configs) | `scripts/setup-hooks.sh` |

## Conventions not in README

- **Branches**: `main` = release-ready, tags cut from here. `develop` = active work. Open PRs against `develop`.
- **Commit subject**: `Verb subject (#N).` — verb-first, trailing period, related GitHub issue in `(#N)`. Examples: `Fix IME Enter on rename input (#1).`, `Add session pinning (#12).`, `Drop unused config flag.` (no issue → no paren). No Conventional-Commit prefixes (`fix:` / `feat:`); no `Fixes #N` footer — the `(#N)` in the subject replaces it.
- **No Claude co-author trailer**: don't append `Co-Authored-By: Claude ... <noreply@anthropic.com>` to commit messages.
