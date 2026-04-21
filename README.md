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

<p align="center">
  <img src="design/chatterm-preview.png" alt="ChatTerm Screenshot">
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

## Install

### One-line install (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash
```

Works on Apple Silicon and Intel Macs (universal binary). Since `curl` doesn't apply the `com.apple.quarantine` attribute that browsers add, the unsigned app launches without Gatekeeper warnings.

### Manual DMG download

Grab the DMG from [Releases](https://github.com/chatterm/chatterm/releases). Because ChatTerm is not yet code-signed, double-clicking a browser-downloaded DMG may fail with "ChatTerm is damaged". Strip the quarantine attribute first:

```bash
xattr -cr ~/Downloads/ChatTerm_*.dmg
```

Then open the DMG and drag ChatTerm to `/Applications`.

## Development

```bash
npm install
npm run tauri dev
```

## Build from source

```bash
npm run tauri build
bash install.sh
```

## Setup Agent Hooks

The hook installer writes `~/.chatterm/hook.sh` and wires it into each agent's config. Pick whichever entry point matches your install:

```bash
# Installed via DMG / curl
bash /Applications/ChatTerm.app/Contents/Resources/setup-hooks.sh

# Installed via one-line curl (also works)
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash

# Running from a repo checkout
bash scripts/setup-hooks.sh
```

All three entry points produce the same result: the hook lives at `~/.chatterm/hook.sh`, and the following configs reference that stable path:

| Agent | Config file | Activation |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | Applies globally — restart Claude Code |
| Codex | `~/.codex/hooks.json` + `config.toml` | Applies globally — restart Codex |
| Kiro CLI | `~/.kiro/agents/chatterm.json` | **Per-agent** — see below |

### Activating the Kiro CLI hooks

Kiro CLI loads hooks from the **active agent profile**, not globally. After running `setup-hooks.sh`, switch to the `chatterm` agent:

```bash
# Start a new Kiro session with the chatterm agent:
kiro-cli chat --agent chatterm

# Or inside an existing session:
/agent swap chatterm
```

To make `chatterm` the default Kiro agent permanently, set it in `~/.kiro/settings.json` (or create a shell alias: `alias kiro-cli='kiro-cli chat --agent chatterm'`).

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

scripts/                    # Install + hook scripts
├── install-remote.sh       # One-line curl installer
└── setup-hooks.sh          # Agent hook installer (writes ~/.chatterm/hook.sh)

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
