# ChatTerm Agent ANSI Capture & Analysis

An offline PTY recorder + ANSI coverage analyzer. It exists to:

- Research what raw bytes a new AI coding-agent CLI actually writes into the terminal.
- Compare different agents' chrome (title / mouse / synchronized updates / permission dialogs).
- Ground chatterm's agent recognition and rendering in **measured data**, not guesswork.

This document has three parts: **findings**, **testing methodology**, and a **new-agent adaptation guide**.

> 中文版见 [`README.md`](README.md)。Both files are kept in sync.

---

## 1. Findings (first sweep, 2026-04-22)

Running the same prompt set ("explain a PTY" + "write fizzbuzz" + "create a file") against three agents produced differences large enough to call **three completely different rendering architectures**.

### 1.1 Three-way comparison

| Dimension                        | Claude Code             | Codex CLI               | Kiro CLI                    |
| -------------------------------- | ----------------------- | ----------------------- | --------------------------- |
| Version                          | 2.1.117                 | codex-cli 0.122.0       | kiro-cli 2.0.1              |
| Bytes for same prompt set        | 32 KB                   | **5.5 KB**              | **97 KB**                   |
| Render model                     | incremental cursor ops  | standards-conservative  | full-frame repaint          |
| OSC 0/2 title                    | yes (stateful)          | yes (cwd + spinner)     | **none at all**             |
| Synchronized update `?2026`      | 0                       | 7 pairs                 | **214 pairs (per frame)**   |
| Cursor toggle `?25`              | 3 balanced              | 6↑ / 1↓                 | 0↑ / 227↓ (**not restored**) |
| Mouse tracking                   | all 4 (1000/2/3/6)      | none                    | none                        |
| Focus events `?1004`             | yes                     | yes                     | no                          |
| Color-scheme notify `?2031`      | yes                     | no                      | no                          |
| Kitty keyboard `CSI u`           | no                      | **yes** (`\e[>7u`)      | no                          |
| DECSTBM scroll region            | no                      | **4 uses**              | no                          |
| Full clear `\e[2J`               | rare                    | rare                    | **105× (per-frame)**        |
| Word separator                   | **`\e[1C` (not space)** | space                   | space                       |
| OSC color query                  | none                    | `\e]10;?;\a` (fg)       | `\e]11;?;\a` (bg)           |
| Other protocols                  | OSC 9 (iTerm growl)     | `\e[6n` DSR × 2         | `\e[6n` DSR × 1             |
| Thinking marker                  | braille prefix in title | braille prefix in title | inline body `⠀ Thinking...` |
| **Thinking spinner frames**      | title `⠂⠐` **2-frame** blink | title `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` **10-frame** rotation | body `⠀⠁⠉⠙⠹⢹⣹⣽⣿` **9-frame** accumulating |
| **Permission dialog**            | no box-drawing           | trust dialog (on startup) | box-drawing                          |
| **"Always allow" mode**          | option 2 → `⏵⏵ accept edits on` | *unconfirmed*    | option 2 → trust (chrome TBC)         |

### 1.2 Claude Code deep-dive

**Title protocol is tiered:**

| State                        | Example                                                   |
| ---------------------------- | --------------------------------------------------------- |
| Startup idle                 | `\e]0;✳ Claude Code\a`                                    |
| Thinking (no task yet)       | `\e]0;⠂ Claude Code\a` / `\e]0;⠐ Claude Code\a` (**only 2 frames** — not a full braille cycle) |
| Working on a task            | `\e]0;⠂ Read hello.txt and search for TODO\a`             |
| Task complete                | `\e]0;✳ Read hello.txt and search for TODO\a`             |

**Gotcha**: after a task completes, the title **never** reverts to `✳ Claude Code` — the last task description stays pinned. So `title == "✳ Claude Code"` is reliable for post-startup idle detection **only for the first task**, never for mid-session idle. Agent-state detection must **combine**:

- title prefix char (`✳` steady / braille cycling = thinking)
- body text `⏺` + `Interrupted · What should Claude do instead?` = waiting-for-redirection
- body text `Do you want to proceed?` / `Do you want to make this edit` = waiting-for-confirmation
- byte-idle threshold as a fallback

**`\e[1C` as word separator**: the permission dialog does **not** put spaces between words — it uses CUF-1 (cursor-forward-1). Raw bytes:

```
Yes,\e[1Cand\e[1Calways\e[1Callow\e[1Caccess\e[1Cto\e[1C<bold>chatterm-capture-sandbox<end-bold>/\e[1Cfrom\e[1Cthis\e[1Cproject
```

Direct impact on chatterm: **sidebar preview extraction** that naively strips CUF sequences will collapse `"Yes,andalwaysallowaccess..."`. The correct approach is to either expand `CUF-N` into N spaces in the screen grid, or extract previews from the 2D representation in `vscreen.rs` rather than the raw byte stream.

**Two permission-dialog variants:**

```
# Bash-class (touch / ls / rm)
Do you want to proceed?
❯ 1. Yes
  2. Yes, and always allow access to <cwd>/ from this project
  3. No
Esc to cancel · Tab to amend · ctrl+e to explain

# Edit-class (preceded by a full inline diff hunk)
Do you want to make this edit to <file>?
❯ 1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No
```

Neither uses box-drawing glyphs — it's all `\e[1C` padding + truecolor SGR + trailing-space fill. The selected option uses lavender `rgb(177,185,249)` for both the `❯` pointer and the text; unselected options are grey `rgb(153,153,153)`.

**Edit-diff uses a Monokai palette with background-color hunks:**

- Added line: bg `rgb(4,71,0)` (dark green) + fg `rgb(80,200,80)` (bright green)
- Removed line: bg `rgb(61,1,0)` (dark red) + fg `rgb(220,90,90)` (dim red)
- **Both sides carry the same Monokai syntax highlighting** — so "does this line have syntax highlighting" does not distinguish the diff direction.

**"Always allow" mode** (second-sweep finding — side effect of option 2): after pressing `2` on a permission dialog, Claude enters a session-wide trust state. **All subsequent tool calls bypass the permission dialog** (confirmed: after approving `touch`, a later `rm` also executed silently). A persistent indicator appears in the bottom status bar:

```
⏵⏵ accept edits on (shift+tab to cycle)
```

`shift+tab` cycles through several modes (only `accept edits on` confirmed in tests). chatterm currently has **no detection** for this signal.

**Title braille is only 2 frames** (second-sweep confirmation): across 5 cases and ~250 KB of data, Claude's title alternates between `⠂` (U+2802) and `⠐` (U+2810) in a blink pattern — **not** a full 8-frame braille rotation. Body spinners are richer (`✢ ✳ ✶ ✻ ✽ ·` paired with a random gerund like `Roosting…` / `Reticulating…` / `Gallivanting…`).

### 1.3 Codex CLI deep-dive

- **Title = cwd name**. Thinking adds a braille prefix cycling through the **full 10-frame clockwise rotation**: `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`. Full example: `\e]0;⠦ chatterm-capture-sandbox\a`.
- **First-run trust dialog**: `Do you trust the contents of this directory?` → `1. Yes, continue / 2. No, quit`. Without acknowledging it with Enter, you cannot reach the normal session (easy fixture trap).
- **Blocks prompt submission until all MCPs load** (second-sweep finding): codex boots all configured MCP servers in parallel; the body shows `Starting MCP servers (N/M): <name>`. Until this clears, any prompt you type goes into an input queue (`tab to queue message`) and **`\r` does not submit**. Fixture ready-regex must watch not just the trust dialog passing but also `Starting MCP servers` disappearing or the status bar `gpt-N.N xhigh · <cwd>` stabilising.
- **Uses the Kitty keyboard protocol**: `\e[>7u` (push flags) / `\e[<1u` (pop flags). This is the xterm `modifyOtherKeys` alternative, enabling unambiguous Ctrl+Shift combo detection. The host terminal **must** support it or key input breaks.
- **Uses DECSTBM scroll regions**: `\e[1;40r` confines scrolling output so the status bar at the bottom is left alone. Classic TUI technique neither Claude nor Kiro uses.
- **Smallest byte footprint**: 5.5 KB for the same three prompts. Codex has very precise region-update strategy.
- **Prompt submit must go through bracketed paste** (second-sweep pitfall): Codex pushes Kitty keyboard flag 7 at startup, which disambiguates Enter away from plain CR. Crossterm under flag 7 does **not** accept raw `\r`; `\e[13u` / `\e[13;1u` / `\e[13;1:1u` and other Kitty-encoded variants also fail. The **only** encoding that submits is wrapping the prompt in bracketed paste: `\e[200~<prompt>\e[201~\r` — crossterm's paste channel is orthogonal to the keystroke path, and the trailing `\r` after paste-end is accepted as submit. Codex fixtures in this tool must therefore wrap each prompt manually; `codex-basic.toml` has the template.

### 1.4 Kiro CLI deep-dive

- **Emits no OSC title at all**. chatterm's `agents.json`, if it only looks at titles, will never recognize Kiro. Identification must go through argv (`command` contains `kiro-cli`) or body-text matching (`Thinking... (esc to cancel)`).
- **`?2026` wraps every frame**: 214 set/reset pairs in 106 seconds ≈ 2 fps. **Entirely dependent on terminal support** (iTerm2 / kitty / WezTerm support it; basic xterm / gnome-terminal do not). On unsupporting terminals you will see mid-frame tearing.
- **Full-frame repaint**: every frame is `\e[2J\e[H` clear + redraw. Easy on Kiro's logic (no local diff) but costs 3× Claude's byte rate.
- **Exit bug**: `?25` (cursor visibility) was reset 227 times and set 0 times. After a clean exit the terminal may be left with a hidden cursor — the next prompt will have an invisible caret. Not ours to fix, but if chatterm **tracks `?25` state internally**, we can force `\e[?25h` on agent-process EOF as a belt-and-suspenders.
- **Does not emit mouse / focus / color-scheme events**. Assumes "a terminal is a terminal" with no capability negotiation.
- **Body spinner is a 9-frame accumulating fill** (second-sweep): `⠀ ⠁ ⠉ ⠙ ⠹ ⢹ ⣹ ⣽ ⣿` (each frame adds more dots, presumably resets when full). A typical body line: `⠀ Thinking... (esc to cancel)` — `⠀` (U+2800 empty braille) is the starting frame, not an ordinary space.
- **Has a permission dialog** (missed in the first sweep, captured in the second — see below).

**Permission dialog (second-sweep capture, correcting the first sweep's "Kiro has no permission dialog" claim):**

```
────────────────────────────────────────
 write requires approval               ← verb varies: write / shell / read / ...
 ❯ Yes, single permission               ← one-shot approval
   Trust, always allow in this session   ← session-scope trust (= Claude's option 2 equivalent)
   No (Tab to edit)                      ← deny + jump into edit mode to rewrite the prompt
────────────────────────────────────────
 ESC to close | Tab to edit
```

- **Box-drawing** (`─`) — opposite styling from Claude's dialog
- Stable primary regex: `\w+ requires approval` (`write` / `shell` observed; other verbs presumed)
- Option 3's `Tab to edit` is unique to Kiro — when the user disagrees with the prompt they can **jump straight into edit mode** and rewrite before retrying, without closing the dialog.

### 1.5 Direct implications for chatterm

| Finding                                          | Action item                                                                            |
| ------------------------------------------------ | -------------------------------------------------------------------------------------- |
| `\e[1C` used as a word separator                 | Preview extraction should go through `vscreen`'s 2D grid, not byte-level chrome stripping |
| Claude title reverts only at startup             | State detector must combine title + body text + byte-idle, not title alone             |
| Kiro has no title                                | `agents.json` needs argv matching and a body-text fallback (`Thinking... (esc to cancel)`) |
| `?2026` wraps every Kiro frame                    | `vscreen` should at least recognize the pattern (double-buffer is optional, awareness is not) |
| Kitty keyboard `CSI u` is Codex's keyboard protocol | Chrome stripper must not treat `\e[>Nu` / `\e[<Nu` as SCORC / SCOSC                    |
| Permission dialog has no box-drawing             | Detect permission prompts via text features: `Do you want to proceed?` / `Do you want to make this edit` |
| Claude Edit-diff uses bg color for add/remove     | Diff-region detection should key off `\e[48;2;...m` bg-color patterns, not `+/-` chars |
| Kiro leaves `?25` unbalanced on exit              | chatterm should emit `\e[?25h` on PTY EOF as a fallback                                |
| Claude title is only a 2-frame blink              | ✅ `agents.json` thinking prefix updated to `["⠂","⠐"]` (cb8307d)                       |
| Codex title's 10 frames are unconfigured          | ✅ `agents.json` now carries `osc_title_prefix: ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]` (cb8307d) |
| Kiro body spinner is a fast-path thinking trigger | Alongside `Thinking\.\.\.`, also trigger on animation chars `⠀/⠁/⠉/⠙/⠹/⢹/⣹/⣽/⣿` (TBD)  |
| Kiro permission dialog has box-drawing variant    | ✅ Rule `\w+ requires approval` landed in `agents.json` asking (cb8307d)                |
| Claude "accept edits on" mode is invisible to us  | New state (or agent badge): match `⏵⏵ accept edits on`, surface "no-confirmation mode" (TBD) |
| Codex prompts require bracketed-paste submit      | ✅ Fixtures wrap with `\e[200~...\e[201~\r`; template in codex-basic.toml (a0c7624)     |

### 1.6 Asking state (landed: cb8307d + d98df25 + 96fedbe + a0c7624)

The original `StateDef` had only `thinking` / `idle`. When a permission dialog appeared, Claude's title was still `✳ <task>` — matching the idle rule — yet the user was blocked. That was **a bug in the rules**, not just a missing feature. `cb8307d` landed an optional `asking` field with priority `asking > thinking > idle`; `96fedbe` made the Sidebar's asking status dot a fast red pulse to make the signal impossible to miss.

The regex as shipped (all three agents' rules empirically verified in live captures):

```jsonc
// Claude
"asking": {
  "screen_regex": [
    "Do you want to proceed\\?",             // Bash-class permission
    "Do you want to make this edit to",      // Edit-class permission
    "What should Claude do instead\\?"       // Redirection wait after deny / Ctrl-C
  ]
}

// Kiro
"asking": {
  "screen_regex": [
    "requires approval",                     // Primary, verb-agnostic (covers write / shell / web_fetch)
    "Yes, single permission",                // Backup 1
    "Trust, always allow in this session"    // Backup 2
  ]
}

// Codex (d98df25 based on a user-supplied screenshot; a0c7624 re-verified via harness capture)
"asking": {
  "screen_regex": [
    "Do you trust the contents of this directory",  // Startup trust dialog
    "Would you like to run the following command",  // Runtime sandbox permission
    "No, and tell Codex what to do differently"     // Option-3 fallback
  ]
}
```

The hook-API `"ask"` event (Codex / Claude hook integration) was also changed from "map to idle" to "map to asking" in cb8307d.

**Unresolved chrome-filter migration**: the `chrome[]` list still contains patterns that make up asking chrome and would be stripped from preview if left there — so when `asking` fires, the Sidebar preview won't show the dialog text the user must see:

- Claude: `"yes, i trust|no, exit"`, `"enter to confirm"`, `"quick safety check"`, `"be able to read, edit"`, `"security guide"`

These should move from chrome to asking.screen_regex (or the preview extractor in `pty.rs` should become asking-aware and skip chrome filtering while asking is active). Not done yet — see "Known issues" below.

---

## 2. Testing methodology

### 2.1 Architecture

```
fixture.toml  ──►  recorder  ──►  PTY (spawn agent)  ──►  reader thread
                      │                                       │
                      │                                       ▼
                      │                                  raw.bin      (byte-exact)
                      │                                  cast.json    (asciinema v2 replayable)
                      │                                  stream.ndjson (per-chunk ts + hex)
                      ▼
                   analyzer (vte::Parser + Perform trait)
                      │
                      ▼
                   coverage.json (bucket counts, first_offset, examples)
                      │
                      ▼
                   report.html (cross-case heatmap + drill-down)
```

Design choices worth calling out:

1. **Byte-exact vs UTF-8 split**. `raw.bin` is the literal `read()` output from the PTY master. Analysis, regression, diff — everything uses it as the source of truth. `cast.json` uses `String::from_utf8_lossy` and is only for `asciinema play` replay.
2. **The analyzer reuses chatterm's own vte crate** (0.15) so the analyzer and the runtime see the same state machine. What the analyzer parses is what `src-tauri/src/vscreen.rs` parses.
3. **`report` reads only `coverage.json`** — it never re-runs fixtures. Real-agent captures burn tokens, take seconds, and are not deterministic, so "run" and "view report" are deliberately separated.

### 2.2 Directory layout

```
capture/
├── Cargo.toml
├── src/
│   ├── main.rs       # CLI dispatch
│   ├── fixture.rs    # TOML schema
│   ├── recorder.rs   # PTY driver + multi-way tee
│   ├── analyzer.rs   # vte classifier
│   └── report.rs     # HTML report
├── fixtures/
│   ├── bash-*.toml           # Baseline fixtures that don't need an agent (SGR, OSC, mouse, ...)
│   └── agents/
│       ├── claude-*.toml     # Real-agent fixtures (only run explicitly)
│       ├── codex-*.toml
│       └── kiro-*.toml
└── artifacts/                # Capture outputs, one dir per case
    └── <agent>/<case>/
        ├── raw.bin
        ├── cast.json
        ├── stream.ndjson
        └── coverage.json
```

**Important**: `capture coverage fixtures/` only scans `.toml` files at the **top level**, it does **not** recurse into `agents/`. This is deliberate — bash fixtures are free (millisecond scale), agent fixtures take minutes and cost tokens, so they must not be swept together.

### 2.3 Commands

```bash
# Run a single fixture (auto-analyzes and prints coverage summary)
cargo run -- run fixtures/bash-sgr.toml
cargo run -- run fixtures/agents/claude-permission.toml

# Batch-run every fixture in a directory
cargo run -- coverage fixtures/           # all bash baselines
cargo run -- coverage fixtures/agents/    # agents (explicit opt-in)

# Re-analyze an existing raw.bin without re-capturing
cargo run -- analyze artifacts/claude/tools/raw.bin

# Generate the combined HTML report
cargo run -- report artifacts/ --out artifacts/report.html
```

### 2.4 Fixture schema

```toml
agent = "codex"                  # identifier; artifacts land under artifacts/<agent>/
case = "permission-approve"      # sub-directory for this scenario under the same agent
command = ["codex"]              # optional, defaults to $SHELL -l
cwd = "/tmp/sandbox"             # optional working dir (useful for side-effect isolation)
cols = 140
rows = 40
idle_ms_default = 8000           # default wait_idle window
timeout_ms = 240000              # hard timeout for one run
env = { NO_COLOR = "0" }         # optional extra env vars

[[step]]
kind = "wait_regex"              # match a regex in the accumulated bytes
pattern = "Do you want"          # note: \xNN byte escapes work because (?-u) is auto-prepended
timeout_ms = 90000

[[step]]
kind = "send"                    # write bytes to the PTY
data = "1<CR>"                   # Named-key tokens supported — see §2.4.1 below

[[step]]
kind = "wait_idle"               # block until ms of byte silence
ms = 8000

[[step]]
kind = "resize"                  # dynamic PTY resize test
cols = 80
rows = 24

[[step]]
kind = "sleep"                   # unconditional wait
ms = 1000
```

**Traps:**

- TOML basic strings **disallow** literal `\x00-\x1F` bytes (except `\t`). **Preferred workaround** is the named-key tokens below; if you insist on raw unicode, use `"\u001B"` for ESC, `"\u0003"` for Ctrl-C, `"\u0004"` for Ctrl-D.
- `wait_regex` uses `regex::bytes` with an automatic `(?-u)` prefix so **byte-level** matching works: `\\xe2\\x9c\\xb3` reliably hits the three UTF-8 bytes of `✳`. If you want to match multibyte characters, hex byte escapes are more robust than literal Unicode.
- `send.data` is **not** double-unescaped (except for named-key tokens). To send ESC for the shell's `printf` to interpret, write `"\\e[31m"` (after TOML parse, becomes the two chars `\e[31m`). To send a real ESC byte, use `"<ESC>[31m"`.

#### 2.4.1 Named-key tokens (recommended)

`send.data` supports the following angle-bracket-wrapped tokens; each expands to its raw byte before the send. These tokens **never appear in natural-language or shell text**, so no false-positive collisions with real prompts:

| Token   | Byte | Meaning                       |
| ------- | ---- | ----------------------------- |
| `<ESC>` | 0x1B | ESC / any CSI leader byte     |
| `<C-c>` | 0x03 | Ctrl-C (interrupt)            |
| `<C-d>` | 0x04 | Ctrl-D (EOT)                  |
| `<CR>`  | 0x0D | Carriage Return               |
| `<LF>`  | 0x0A | Line Feed                     |
| `<TAB>` | 0x09 | Tab                           |
| `<BS>`  | 0x08 | Backspace                     |

Examples: close dialog `<ESC>`, interrupt `<C-c>`, exit TUI `<C-d>`, explicit newline `<LF>`.

#### 2.4.2 Codex-specific: bracketed-paste submit

**Codex pushes Kitty keyboard flag 7 at startup**, which means plain `\r` through the keystroke path will not submit (see §1.3). Every prompt in a Codex fixture therefore needs to be wrapped in bracketed paste:

```toml
[[step]]
kind = "send"
data = "<ESC>[200~say hi in one word<ESC>[201~\r"
```

Crossterm's paste event channel is orthogonal to the keystroke path, and the trailing `\r` after paste-end is accepted as submit. Claude / Kiro don't need this — `"say hi\r"` works directly. `codex-basic.toml` and `codex-permission-matrix.toml` are the reference templates.

### 2.5 ANSI bucket taxonomy

`analyzer.rs` classifies every vte event into one of these 20 buckets:

| Bucket                     | Events                                                |
| -------------------------- | ----------------------------------------------------- |
| `print`                    | Printable characters                                  |
| `c0`                       | C0 controls (BEL / BS / HT / LF / CR / FF / …)        |
| `csi_sgr`                  | CSI `m` (color, style)                                |
| `csi_cursor`               | CSI `A/B/C/D/E/F/G/H/f/d/s/u`                         |
| `csi_erase`                | CSI `J/K/X` (ED / EL / ECH)                           |
| `csi_edit`                 | CSI `@/L/M/P` (ICH / IL / DL / DCH)                   |
| `csi_mode_set`             | CSI `h` (DEC / ANSI mode set)                         |
| `csi_mode_reset`           | CSI `l`                                               |
| `csi_scroll`               | CSI `r/S/T` (DECSTBM / SU / SD)                       |
| `csi_report`               | CSI `n/c` (DSR / DA)                                  |
| `csi_other`                | Other CSI finals (TBC / CHT / CBT / …)                |
| `osc_title`                | OSC 0/1/2                                             |
| `osc_color`                | OSC 4 / 10 / 11 / 12 / 17 / 19 / 104 / 110 / 111      |
| `osc_cwd`                  | OSC 7                                                 |
| `osc_hyperlink`            | OSC 8                                                 |
| `osc_clipboard`            | OSC 52                                                |
| `osc_shell_integration`    | OSC 133 (iTerm / VSCode prompt marks) / 633 / 697      |
| `osc_other`                | Any other OSC (this sweep produced OSC 9 growl)        |
| `dcs`                      | DCS (sixel, tmux passthrough, …)                      |
| `esc_other`                | Single-char ESC sequences (DECSC `\e7` / DECRC `\e8`) |

Plus three cross-cutting tables: `decset_modes` (set/reset counts per private mode number), `osc_codes` (count per OSC code), `sgr_attrs` (count per SGR attribute number — **known minor bug**: RGB subparameters of `\e[38;2;R;G;Bm` are also counted as standalone SGR codes).

---

## 3. Adapting a new agent CLI

Full workflow for wiring up a new agent. Examples use a hypothetical `acme-cli`.

### Step 1: Confirm it is an interactive TUI

```bash
acme-cli --help          # look for an interactive / chat subcommand
which acme-cli           # confirm the path exists
acme-cli                 # run it; observe whether it exits immediately or enters a TUI
```

If it's an IDE launcher (e.g., `kiro` itself = IDE launcher; `code` = VSCode), **it's out of scope for this tool**. The target must be a foreground CLI that reads stdin and writes stdout interactively.

### Step 2: Write a minimal startup fixture (wait_idle only)

Goal: **make it run**, with zero assumptions about chrome.

```toml
# fixtures/agents/acme-basic.toml
agent = "acme"
case = "basic"
command = ["acme-cli"]
cwd = "/tmp/chatterm-capture-sandbox"
cols = 140
rows = 40
idle_ms_default = 8000
timeout_ms = 240000

[[step]]
kind = "wait_idle"
ms = 4000

[[step]]
kind = "send"
data = "hello\r"

[[step]]
kind = "wait_idle"
ms = 20000

[[step]]
kind = "send"
data = ""            # Ctrl-D fallback exit
```

Run:

```bash
cargo run -- run fixtures/agents/acme-basic.toml
```

**Common startup problems:**

| Symptom                                  | Cause                                                   | Fix                                                          |
| ---------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------ |
| `write: Input/output error (os error 5)` | Agent process already exited (banner-only / bad args)   | Check the tail of `raw.bin` plain text, usually usage error  |
| Very few bytes, no visible interaction   | Agent needs login (`*-cli login` or browser OAuth)      | Log in before running                                        |
| Hangs on startup                         | A boot-time dialog (trust / TOS / accept)               | Add a `send = "\r"` step to acknowledge                      |
| No PTY output                            | Agent detects non-tty or small screen, goes batch-mode  | Bump cols/rows                                               |

### Step 3: Inspect `raw.bin` to find chrome markers

Essential one-liners after a first run:

```bash
# Extract OSC 0 title transitions
python3 -c "
import re
from itertools import groupby
data = open('artifacts/acme/basic/raw.bin','rb').read()
titles = re.findall(rb'\x1b\]0;([^\x07]*)\x07', data)
for t in [k.decode(errors='replace') for k,_ in groupby(titles)]:
    print(repr(t))
"

# Strip all CSI/OSC to see plain text
python3 -c "
import re
data = open('artifacts/acme/basic/raw.bin','rb').read()
s = re.sub(rb'\x1b\[[0-9;?]*[a-zA-Z]', b'', data)
s = re.sub(rb'\x1b\][^\x07]*\x07', b'', s)
print(s.decode(errors='replace'))
" | less

# See category distribution
cat artifacts/acme/basic/coverage.json | jq '.categories | keys'
cat artifacts/acme/basic/coverage.json | jq '.decset_modes'

# Eyeball replay
asciinema play artifacts/acme/basic/cast.json
```

Focus on:

- **OSC title protocol**: does it exist? does it change with state? does it have a spinner prefix? This determines whether title can drive state detection.
- **Presence of `?2026`**: if yes, the agent relies on synchronized updates and chatterm's renderer must handle it.
- **Stable text marker for permission / confirmation dialogs**: find a sentence that doesn't change with the specific prompt (e.g., `Do you want to proceed?`).
- **Thinking / waiting marker**: may not be in the title — could be body text like Kiro's `Thinking... (esc to cancel)`.
- **Kitty keyboard flag push (`\e[>Nu`)**: if you see this sequence in the startup bytes (e.g., Codex's `\e[>7u`), the agent is very likely using crossterm/ratatui with progressive keyboard enhancement — **plain `\r` may not register as Enter**. If your first prompt step never triggers a title animation, this is almost certainly why. Fix: wrap prompts in bracketed paste (`<ESC>[200~...<ESC>[201~\r`), see §2.4.2.

### Step 4: Tighten with `wait_regex`

`wait_idle` is the safety net, but production-quality fixtures use `wait_regex` to lock onto state transitions:

```toml
# Wait for startup to complete
[[step]]
kind = "wait_regex"
pattern = "\\xe2\\x9c\\xb3 Acme"      # ✳ Acme — the startup-idle title
timeout_ms = 20000

# Wait for it to start thinking
[[step]]
kind = "wait_regex"
pattern = "Thinking\\.\\.\\."
timeout_ms = 10000

# Wait for a permission dialog
[[step]]
kind = "wait_regex"
pattern = "Do you want"
timeout_ms = 90000
```

**Rules of thumb:**

- Real agents have high network/thinking variance — **use 60-90s timeouts** to stay stable.
- Keep each regex short and unique. Longer sentences break more easily under interleaved SGR / CUP.
- Hex byte escapes `\\xNN` are the most reliable — a literal Unicode char in the pattern will round-trip through TOML's UTF-8, which usually aligns with the raw stream, but hex is explicit.
- The cursor advances on each match, so using the same pattern in consecutive waits will match successive occurrences.

### Step 5: Cover tools and branches

Once the baseline fixture is stable, split scenarios by what chatterm needs to verify:

```
fixtures/agents/
├── acme-basic.toml        # Smoke: starts, talks, exits
├── acme-permission.toml   # Permission dialog — approve path
├── acme-deny.toml         # Permission dialog — deny path
├── acme-tools.toml        # Read / Write / Grep / other tool invocations
├── acme-interrupt.toml    # Ctrl-C interruption path
└── acme-long-output.toml  # Long streaming output (scroll region / alt-screen behavior)
```

One intent per fixture makes it easy to localize regressions when the chrome changes.

### Step 6: Fold findings into `src-tauri/agents.json`

The end goal is chatterm recognizing the agent. Per §1.5, you need at minimum:

- **argv match**: `command` contains `acme-cli` → identify as acme
- **title match** (if the agent uses titles): regex for startup / thinking / idle titles
- **state rules**: per-state (idle / thinking / waiting-confirm / awaiting-input) trigger features
- **chrome filters**: which areas are the agent's own UI and should be skipped during preview extraction

### Step 7: Regression baseline

When an agent version bumps, re-run the fixtures and diff `coverage.json`. **Counts changing by >20%, or new categories appearing**, means the agent's rendering behavior shifted and chatterm might need a follow-up.

```bash
# Structural diff
diff <(jq -S '.categories | keys' artifacts/acme/basic/coverage.json) \
     <(jq -S '.categories | keys' artifacts/acme/basic-new/coverage.json)

# Count-level diff (subjective threshold)
jq -r '.categories | to_entries | .[] | "\(.key) \(.value.count)"' \
  artifacts/acme/basic/coverage.json
```

A `capture diff` subcommand to automate this is planned; for now it's jq + diff.

---

## 4. Known issues / future work

- `sgr_attrs` conflates truecolor RGB subparameters with standalone SGR codes, inflating the numbers. Fix: after encountering 38/48 in the param list, skip the following color subparams.
- `csi_cursor` lumps in Kitty keyboard protocol's `\e[>Nu` / `\e[<Nu` (final byte `u`). They should have their own `csi_keyboard` bucket.
- No `capture diff` / `capture baseline` subcommand yet for regression assertions. `jq` works but is tedious.
- `cast.json` goes through `String::from_utf8_lossy`, so a rare UTF-8 split across a chunk boundary can show `�`. Fine for demo replay, not a source of truth for analysis.
- Agent fixtures produce real side effects (creating files in cwd, editing files, calling APIs). Always run in a disposable sandbox directory.
- Codex fixtures must manually wrap each prompt in bracketed paste (`<ESC>[200~...<ESC>[201~\r`). A nicer approach would be for the recorder to auto-wrap when `agent = "codex"`, but that isn't implemented — explicit wrapping keeps the rule visible.
- `chrome[]` still contains several trust / safety-dialog texts that get stripped from preview (see the tail of §1.6). Under `asking`, the Sidebar preview therefore shows nothing. Fixing this needs a change in `pty.rs` preview extraction (make it asking-aware and skip chrome filtering while asking is active) or a migration of those patterns into `asking.screen_regex`.
