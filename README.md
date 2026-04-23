# ChatTerm

<p align="center">
  <img src="design/logo/v4/1D_bubble_gradient_256.png" width="128" alt="ChatTerm">
</p>

<p align="center">
  <strong>Built for AI coding sessions.</strong><br>
  An IM-style terminal workspace that manages terminal sessions like chat conversations.
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="design/chatterm-preview.png" alt="ChatTerm Highlights">
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
- **Theme system** — Built-in ChatTerm / VS Code Dark / Dark+ themes; macOS can import Terminal profiles
- **Session persistence** — Restores session list on restart; agents resume with `--resume`
- **⌘K search** — Quick session search by name, cwd, or output
- **Shell preview** — Shows last command and working directory for shell sessions

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite 7 + xterm.js
- **Backend**: Rust + Tauri 2 + portable-pty
- **IPC**: Named Pipe (FIFO) for hook → app communication
- **Theme**: Configurable themes; macOS-only import from Terminal `.terminal` profiles

## Install

ChatTerm itself is one binary; to get the sidebar's reply previews and thinking/idle indicators you also need **agent hooks** wired up. Read the next section once, then pick an install option — each option shows its install command and the matching hook-setup command together.

### How agent hooks work

Claude Code, Kiro CLI, and Codex each fire user-defined commands on events like `Stop`, `PreToolUse`, `PostToolUse`. ChatTerm plugs into those hooks to know when to update the sidebar preview and status dot.

The installer (`setup-hooks.sh`) does two things:

1. Writes a single Python hook to the stable path `~/.chatterm/hook.sh`. It reads the event JSON on stdin and writes a short message to a FIFO at `~/.chatterm/hook.pipe`. ChatTerm's Rust backend reads the FIFO and updates the session UI. The hook uses `O_NONBLOCK` on the FIFO, so if ChatTerm is not running it simply drops the message and does not block the agent.
2. Patches each agent's config file to point at `~/.chatterm/hook.sh`:

    | Agent | Config file | Activation |
    |---|---|---|
    | Claude Code | `~/.claude/settings.json` | Global — restart Claude Code |
    | Codex | `~/.codex/hooks.json` + `config.toml` | Global — restart Codex |
    | Kiro CLI | `~/.kiro/agents/chatterm.json` | **Per-agent** — see [Activating Kiro CLI hooks](#activating-kiro-cli-hooks) |

The installer is idempotent: re-running it refreshes stale ChatTerm entries and leaves unrelated hooks alone.

---

### Option 1 — One-line install (macOS and Debian/Ubuntu)

```bash
# 1. Install the app (auto-detects macOS vs Linux)
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash

# 2. Register agent hooks
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash
```

On **macOS** the installer grabs the universal DMG (arm64 + x86_64) and copies `ChatTerm.app` to `/Applications`. Using `curl` avoids the `com.apple.quarantine` attribute a browser would add, so the unsigned app launches without Gatekeeper warnings.

On **Debian / Ubuntu** the installer grabs the arch-matching `.deb` and runs `sudo dpkg -i` + `sudo apt-get install -f` (it will prompt for the sudo password).

**Windows users**: the one-line script is bash-only — use Option 2 below to grab the `.msi` or `.exe` installer and run `setup-hooks.ps1` from PowerShell.

### Option 2 — Manual download

Pick an asset from [Releases](https://github.com/chatterm/chatterm/releases) and run the corresponding commands.

**macOS DMG** — because ChatTerm is not yet code-signed, a browser-downloaded DMG may fail with "ChatTerm is damaged". Strip the quarantine attribute first:

```bash
xattr -cr ~/Downloads/ChatTerm_*.dmg
# open the DMG, drag ChatTerm to /Applications, then:
bash /Applications/ChatTerm.app/Contents/Resources/setup-hooks.sh
```

**Debian / Ubuntu `.deb`**:

```bash
sudo dpkg -i ./chatterm_*.deb
sudo apt-get install -f
bash /usr/lib/chatterm/resources/setup-hooks.sh
```

**Other Linux (`.AppImage`)**:

```bash
chmod +x ./ChatTerm_*.AppImage
./ChatTerm_*.AppImage
# AppImage does not ship the setup-hooks resource; use the remote form:
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash
```

**Windows MSI or NSIS `.exe`** (from [Releases](https://github.com/chatterm/chatterm/releases)):

The two installers drop ChatTerm in different places. MSI is per-machine under `Program Files`; NSIS is per-user under `%LOCALAPPDATA%` by Tauri default. Pick the matching PowerShell command for the one you installed.

```powershell
# MSI (ChatTerm_*_x64_en-US.msi) — per-machine
powershell -ExecutionPolicy Bypass -File "$env:ProgramFiles\ChatTerm\resources\setup-hooks.ps1"

# NSIS (ChatTerm_*_x64-setup.exe) — per-user (default)
powershell -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\ChatTerm\resources\setup-hooks.ps1"
```

The `.ps1` writes the hook at `%APPDATA%\chatterm\hook.py` and wires it into `~/.claude/settings.json`, `~/.kiro/agents/chatterm.json`, and `~/.codex/hooks.json`. The hook relays events to ChatTerm over a Named Pipe (`\\.\pipe\chatterm-hook`), the Windows equivalent of the macOS/Linux FIFO.

### Option 3 — Build from source (repo checkout)

```bash
git clone https://github.com/chatterm/chatterm.git && cd chatterm
npm install

# macOS
npm run tauri build && bash install.sh

# Debian / Ubuntu
npm run tauri -- build --bundles deb,appimage && bash scripts/install-linux.sh

# Windows (produces both .msi and NSIS .exe under src-tauri\target\release\bundle\)
npm run tauri -- build --bundles msi,nsis
# After the produced installer runs:
powershell -ExecutionPolicy Bypass -File scripts\setup-hooks.ps1

# Hook setup on macOS / Linux
bash scripts/setup-hooks.sh
```

For live development (Vite + Tauri dev server):

```bash
npm run tauri dev
```

**Ubuntu dev dependencies** (install once before `tauri dev` / `tauri build`):

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file wget \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

### Activating Kiro CLI hooks

Kiro CLI loads hooks from the **active agent profile**, not globally. After running `setup-hooks.sh`, switch to the `chatterm` agent:

```bash
# Start a new Kiro session with the chatterm agent:
kiro-cli chat --agent chatterm

# Or inside an existing session:
/agent swap chatterm
```

To make `chatterm` the default Kiro agent permanently, set it in `~/.kiro/settings.json` (or alias: `alias kiro-cli='kiro-cli chat --agent chatterm'`).

### Recommended: install a Nerd Font

If your shell prompt uses Powerline / Devicon glyphs (starship, powerlevel10k, oh-my-zsh themes, etc.), install a Nerd Font so those icons render instead of empty boxes. ChatTerm's xterm prefers Nerd Font variants automatically when one is available.

**Linux (Debian/Ubuntu)**

```bash
mkdir -p ~/.local/share/fonts
cd /tmp
wget https://github.com/ryanoasis/nerd-fonts/releases/download/v3.2.1/JetBrainsMono.zip
unzip -o JetBrainsMono.zip -d ~/.local/share/fonts/JetBrainsMonoNerd
fc-cache -fv ~/.local/share/fonts
```

**macOS (Homebrew)**

```bash
brew install --cask font-jetbrains-mono-nerd-font
```

Restart ChatTerm after install.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| ⌘K (macOS) / Ctrl+Shift+K (Linux) | Search sessions |
| ⌘N (macOS) / Ctrl+Shift+N (Linux) | New session |
| F11 | Toggle fullscreen |
| Esc | Close overlays |

## Architecture

```
src/                        # Frontend (React + TypeScript)
├── App.tsx                 # Main app, session state, PTY integration
├── XtermPane.tsx           # xterm.js terminal rendering
├── Sidebar.tsx             # Session list with status indicators
├── CmdK.tsx                # ⌘K search overlay
├── themes.ts               # Theme system
├── types.ts                # Shared types
└── Icons.tsx               # SVG icons

src-tauri/src/              # Backend (Rust)
├── lib.rs                  # Tauri commands, FIFO IPC listener
├── pty.rs                  # PTY manager, agent detection, vscreen
├── vscreen.rs              # Virtual screen for state detection
├── agent_config.rs         # Config-driven agent matching (agents.json)
├── theme.rs                # Theme parser; macOS Terminal import when available
├── session.rs              # Session metadata persistence
└── main.rs                 # Entry point

scripts/                    # Install + hook scripts
├── install-remote.sh       # macOS release installer
├── install-linux.sh        # Linux local bundle installer
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
- [x] Theme system with macOS Terminal import on macOS
- [x] Session persistence and agent resume
- [x] ⌘K search, pin, rename, close

## Target Users

- AI CLI power users (Claude Code / Kiro / Codex)
- Embedded / IoT engineers
- Remote Linux / SSH developers
- DevOps / SRE / Platform engineers

## License

[MIT](LICENSE)
