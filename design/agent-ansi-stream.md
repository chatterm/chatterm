# Agent ANSI Stream Notes

Complements [`ai-cli-patterns.md`](ai-cli-patterns.md) (screen content patterns)
with **stream-level** behaviour — what the three supported agents actually emit
over the PTY, and how `capture/scripts/extract_replies.py` recovers per-turn
replies from those bytes.

Source of truth is the fixtures under `capture/fixtures/agents/` and their
captured artifacts under `capture/artifacts/`. Findings below are derived from
replaying those artifacts through `pyte` + ad-hoc byte inspection. The script
is the executable spec; this doc explains the *why*.

---

## 1. What the pipeline needs from a stream

For each user prompt that produces agent output, the extractor wants:

- **Prompt text** — what the user asked
- **Reply text** — what the agent rendered in response (stripped of TUI chrome)
- **Turn kind** — is this a normal reply or a permission dialog asking the user to approve/deny?
- **Attribution** — which prompt produced which reply, even when multiple are in-flight

Hook-based bridges deliver these fields as structured JSON. Parsing raw ANSI
only has to match that quality when hooks aren't installed or available.

---

## 2. Per-agent behaviour

### 2.1 Claude Code

**Transport:** alt-screen + cursor positioning. Content that scrolls off the
top goes into the terminal's scrollback. Live repaints happen inside the
visible region.

**State signal:** OSC 2 title.

| Title prefix   | State      |
|----------------|------------|
| `✳ …`          | idle       |
| `⠂ …` / `⠐ …`  | thinking (two-frame braille spinner) |
| (any other)    | unknown (rare) |

Asking (permission dialog) has no distinct title — it's detected by screen
regex (`^❯ 1\. Yes`, `^Esc to cancel · Tab to amend`, `^⏺ Interrupted`).

**Reply prefix:** each assistant reply line begins with `⏺ `.

**Quirks observed in captures:**

- Claude only updates the title on the **first** thinking cycle after the
  cwd/session is established. Subsequent titles re-use the first prompt's
  subject (e.g. `⠂ Explain cache coherence concept`) — we can't key off the
  subject string.
- When a user fires several prompts faster than Claude streams replies, Claude
  **batches them into a single thinking cycle** and emits multiple `⏺` blocks
  at once. `claude/chat` prompts 2–5 end up as one batched cycle (visible as
  `idle→idle+split` transitions in the extractor output); the `⏺`-marker
  split restores per-prompt attribution.
- A trailing prompt whose reply lands after the final state transition gets
  attributed to whatever the last submit was — in `claude/chat` that means
  prompt 6's reply ends up in the `eof` tail labeled with the `/exit` prompt
  rather than the summarize prompt. Acceptable for now; would need an
  `eof`-with-pending-submits fixup to attribute correctly.
- Dialog text and post-approval reply share one user submit. The dialog rows
  are overwritten when Claude paints the applied diff, so by the time we
  snapshot, only the post-approval state remains — not a bug, a consequence
  of alt-screen repaint timing.

### 2.2 Kiro CLI

**Transport:** full-screen TUI (not alt-screen). Output stays visible across
turns; nothing scrolls into scrollback unless it overflows the viewport.

**State signal:** none via OSC title (empty throughout). Screen regex only:

| Screen regex             | State       |
|--------------------------|-------------|
| `Thinking…` / `Kiro is working` | thinking |
| `\w+ requires approval` / `Yes, single permission` | asking |
| `^Kiro · `               | idle (footer) |

**Reply prefix:** none. Reply text is just… text, indented by 2 spaces
inside the conversation pane.

**Quirks:**

- Screen-based idle detection **flickers mid-stream**: the "Kiro is working"
  marker briefly disappears during redraws, which the idle regex picks up
  as a transient idle. Without debouncing, a single long reply gets split
  into two turns. Extractor requires `IDLE_DEBOUNCE = 3` consecutive idle
  reads before committing.
- Because Kiro isn't alt-screen, each turn's visible region overlaps with
  the previous turn's visible region. Per-turn display rows are emitted with
  a "dedup against current-turn history only" rule so dialog boilerplate
  like `1. Yes / 3. No` survives across turns, but cross-turn duplicates
  can still appear. Content is correct, just sometimes redundant.
- Large ASCII splash (`⢀⣴⣶⣶⣦⡀ …` KIRO banner) persists in the visible area
  into the first reply. Chrome filter has a dedicated braille-run rule
  (`[⠀-⣿]{10,}`) to drop it.

### 2.3 Codex

**Transport:** alt-screen. Both user prompts and agent replies are echoed
back as first-class rendered lines (not just keystrokes).

**State signal:** OSC title spinner (`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏ …`), but
detection is **unreliable** — the idle regexes in `agents.json`
(`^› $`, `gpt-.*xhigh.*~/`) do not fire consistently on replay. In
practice we abandon state transitions for Codex.

**Markers:**

| Prefix | Meaning          |
|--------|------------------|
| `› `   | User prompt echo |
| `• `   | Assistant reply  |
| `  └ ` | Tool detail under a `•` line |

**Quirks:**

- The `›` prompt echo can be **wiped mid-redraw**. `codex/chat` prompt 3
  was sent and got a reply, but the `› which fits…` echo was cleared by
  a subsequent input-area repaint before we sampled — so the reply was
  orphan by marker-based pairing alone.
- Long prompts **wrap across rows** in the pyte screen. A 180-char prompt
  becomes `› <first 140 chars>` + `  <remainder>`. The extractor merges
  indented continuation rows into the prompt until a blank line or `•`
  marker.
- User-submit write events are the **only reliable prompt source**. The
  Codex extractor anchors turns to writes (with `\r`), then attributes the
  new `• ` rows that appear between this submit and the next.

---

## 3. Extraction algorithm (summary)

Three agent-specific paths inside `extract_replies.py`:

**Claude / Kiro (shared state-transition path)**
1. Replay stream through `pyte.HistoryScreen(140, 40, history=8000)`.
2. Detect state per read: title spinner → thinking; title `✳` / screen `Kiro ·`
   → idle; screen regex → asking.
3. On `thinking → idle` (debounced), queue a `pending_cut`.
4. Commit the cut on: next state change, next user submit, or EOF.
5. `saw_asking_this_cycle` upgrades the turn's kind to `dialog`.
6. Commit any pending cut when the next user submit arrives (write with
   `\r`). Each submit also enqueues its prompt onto `pending_submits` so a
   later batched commit can attribute its split-out blocks back to the right
   prompt.
7. If the cut's delta contains more `⏺` markers than the pending cut has
   submits attached, split the delta at marker boundaries and pair each
   block with the next queued prompt (Claude's rapid-submit batching).

**Codex (marker path)**
1. Feed the entire stream into `pyte.HistoryScreen`. We don't track state
   per-read — only submits matter.
2. At each user-submit event, record the prompt and the *current* count of
   `• `-prefixed rows known to the screen.
3. After replay, walk the submits in order. For submit N, the reply = the
   bullet rows between bullet-count-at-N and bullet-count-at-N+1, expanded
   forward to include continuation rows up to the next `•` (outside this
   range) or `›` (next prompt echo).
4. Prompt text comes from the write bytes (bracketed-paste unwrapped),
   not the screen echo.

**Shared tail**
- First-idle absorption: everything rendered up to the first stable idle
  is startup chrome; add to the global `seen_lines` dedup set but don't emit.
- Chrome filter layered on top of the per-agent `chrome` regex list in
  `agents.json`, extended with a few extra patterns for splash/status-bar
  glyphs that escape the anchored rules after screen redraws.

---

## 4. Known gaps & open questions

- **Claude dialog text loss** when a submit triggers both a permission
  dialog and a post-approval reply: the dialog gets painted over before
  we snapshot. Acceptable under the current per-submit turn semantics
  (one submit = one turn), but downstream consumers wanting the dialog
  text itself would need an intra-turn sub-split.
- **Kiro cross-turn visual overlap** (non-alt-screen): fixable by a global
  seen-set on display rows instead of current-turn-only, but that gutted
  dialog boilerplate when we tried it. Revisit if a turn-boundary-aware
  dedup scheme becomes worthwhile.
- **Codex state regexes in `agents.json`** never fire during replay. Either
  remove them or find working patterns — they're currently dead weight for
  the extraction path (the main app may still use them differently).
- **Hook path coexistence**: if a hook bridge is installed, its structured
  fields should beat the ANSI parse. Decision not made yet on how to
  fuse or fall back. Simplest: prefer hook, use ANSI as backfill for
  hookless sessions.

---

## 5. Reproducing

```
# Capture a fresh fixture (real agent, token spend):
capture/target/release/capture run capture/fixtures/agents/claude-chat.toml

# Extract replies from an existing artifact:
python3 capture/scripts/extract_replies.py capture/artifacts/claude/chat

# JSON output for downstream pipelines:
python3 capture/scripts/extract_replies.py capture/artifacts/claude/chat --json
```

Current fixtures with known-good extraction (15/15 as of 2026-04-27):

| Agent  | Fixtures |
|--------|----------|
| Claude | always-allow, chat, mixed-probe, permission-approve, permission-matrix, spinner-enum, tools |
| Kiro   | basic, long-reply, multi-turn, permission, permission-matrix |
| Codex  | chat, long-reply, multi-turn |
