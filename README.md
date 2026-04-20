# ChatTerm

<p align="center">
  <img src="design/logo/v4/1C_bubble_blue_256.png" width="128" alt="ChatTerm">
</p>

<p align="center">
  <strong>Built for AI coding sessions.</strong><br>
  An IM-style terminal workspace that manages terminal sessions like chat conversations.
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

## Why ChatTerm?

Running multiple AI coding agents (Claude Code, Kiro CLI, Codex), SSH sessions, builds, and logs in parallel is painful with traditional terminals:

- Too many tabs/windows, high switching cost
- No idea which session has new output
- AI agent sessions mixed with shells, logs, builds
- Weak session restore — context lost on restart
- No AI-native interaction design

ChatTerm solves this with an **IM-style session layer** on top of a real terminal.

## Features

- **IM-style sidebar** — Sessions as chat conversations with avatars, previews, and unread badges
- **Agent auto-detection** — Recognizes Claude Code, Kiro CLI, Codex; updates avatar and status
- **Real-time status** — Thinking/idle detection via vscreen pattern matching
- **Hook-driven previews** — Agent reply previews via Named Pipe (FIFO) IPC, no screen scraping
- **Theme system** — Import themes from macOS Terminal, built-in ChatTerm / VS Code Dark / Dark+
- **Session persistence** — Restores session list on restart; agents resume with `--resume`
- **⌘K search** — Quick session search by name, cwd, or output
- **Shell preview** — Shows last command and working directory for shell sessions

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite 7 + xterm.js
- **Backend**: Rust + Tauri 2 + portable-pty
- **IPC**: Named Pipe (FIFO) for hook → app communication
- **Theme**: Configurable, imports macOS Terminal `.terminal` profiles

## Quick Start

```bash
npm install
npm run tauri dev
```

## Build & Install

```bash
npm run tauri build
bash install.sh
```

## Setup Agent Hooks

```bash
bash scripts/setup-hooks.sh
```

Configures hooks for Claude Code (`~/.claude/settings.json`), Kiro CLI (`~/.kiro/agents/chatterm.json`), and Codex (`~/.codex/hooks.json`) to send notifications via FIFO pipe.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| ⌘K | Search sessions |
| ⌘N | New session |
| Esc | Close overlays |

## Architecture

```
src/                        # Frontend (React + TypeScript)
├── App.tsx                 # Main app, session state, PTY integration
├── XtermPane.tsx           # xterm.js terminal rendering
├── Sidebar.tsx             # Session list with status indicators
├── CmdK.tsx                # ⌘K search overlay
├── themes.ts               # Theme system (import macOS Terminal themes)
├── types.ts                # Shared types
└── Icons.tsx               # SVG icons

src-tauri/src/              # Backend (Rust)
├── lib.rs                  # Tauri commands, FIFO IPC listener
├── pty.rs                  # PTY manager, agent detection, vscreen
├── vscreen.rs              # Virtual screen for state detection
├── agent_config.rs         # Config-driven agent matching (agents.json)
├── theme.rs                # macOS Terminal theme parser
├── session.rs              # Session metadata persistence
└── main.rs                 # Entry point

scripts/                    # Hook scripts
├── chatterm-hook.sh        # Hook → FIFO bridge (all agents)
└── setup-hooks.sh          # One-click hook installation

design/                     # Design assets
├── logo/                   # Logo exports (v3, v4, 6 variants)
└── src/                    # UI prototype (JSX)
```

## Status

- [x] IM-style session list with avatars and status
- [x] PTY terminal with xterm.js
- [x] Agent auto-detection (Claude, Kiro, Codex)
- [x] Hook-driven preview via FIFO IPC
- [x] Theme system with macOS Terminal import
- [x] Session persistence and agent resume
- [x] ⌘K search, pin, rename, close

## Target Users

- AI CLI power users (Claude Code / Kiro / Codex)
- Embedded / IoT engineers
- Remote Linux / SSH developers
- DevOps / SRE / Platform engineers

## License

MIT
