#!/usr/bin/env python3
"""
Parse a capture artifact's raw ANSI stream and extract per-turn LLM replies.

Usage:
    scripts/extract_replies.py <artifact_dir> [--agent claude|kiro|codex]
                              [--cols N] [--rows N] [--json]

Reads `stream.ndjson` (timestamped reads + writes) from the artifact dir and
replays the byte stream through a VT100 virtual screen with scrollback (pyte).
Snapshots the screen each time the agent transitions from `thinking` → `idle`
(detected via OSC title spinner + screen regex from agents.json) — that is the
moment a reply is stable. Each snapshot is attributed to the most recent user
submit event (write containing `\\r`) so the output reads as `{prompt, reply}`
pairs.

Output is meant for eyeballing extraction quality on captured fixtures; the
algorithm mirrors what the main app's `agent_config.rs` does, but with
a heavier chrome filter appropriate for post-hoc dumps.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

import pyte


# -------- agent config loader ------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


@dataclass
class StateDef:
    osc_title_prefix: list[str] = field(default_factory=list)
    screen_regex: list[re.Pattern] = field(default_factory=list)


@dataclass
class AgentConfig:
    id: str
    thinking: StateDef
    asking: StateDef
    idle: StateDef
    chrome: list[re.Pattern]


def _compile_list(patterns: list[str]) -> list[re.Pattern]:
    return [re.compile(p) for p in patterns]


def _state(d: dict | None) -> StateDef:
    d = d or {}
    return StateDef(
        osc_title_prefix=d.get("osc_title_prefix", []),
        screen_regex=_compile_list(d.get("screen_regex", [])),
    )


def load_agents() -> dict[str, AgentConfig]:
    path = REPO_ROOT / "src-tauri" / "agents.json"
    data = json.loads(path.read_text())
    out: dict[str, AgentConfig] = {}
    for a in data["agents"]:
        state = a.get("state", {})
        out[a["id"]] = AgentConfig(
            id=a["id"],
            thinking=_state(state.get("thinking")),
            asking=_state(state.get("asking")),
            idle=_state(state.get("idle")),
            chrome=_compile_list(a.get("chrome", [])),
        )
    return out


# -------- state detection ----------------------------------------------------

def detect_state(title: str, screen_lines: list[str], cfg: AgentConfig) -> str:
    """Title-based signals beat screen-based signals — the title is a
    side-channel from the agent, while screen text can lag behind a redraw
    (e.g. a `❯ 1. Yes` dialog row still visible on screen while the spinner
    title says the agent is already thinking again).

    Priority:
      1. title says thinking (spinner prefix)    → thinking
      2. title says idle (idle prefix)           → idle        (but let asking win if a live dialog is on screen)
      3. screen shows asking dialog              → asking
      4. screen shows thinking marker            → thinking
      5. screen shows idle marker                → idle
      6. else                                    → unknown
    """
    stripped = [ln.lstrip() for ln in screen_lines]

    def any_match(pats: list[re.Pattern]) -> bool:
        return any(p.search(line) for line in stripped for p in pats)

    title_thinking = any(title.startswith(p) for p in cfg.thinking.osc_title_prefix) if title else False
    title_idle = any(title.startswith(p) for p in cfg.idle.osc_title_prefix) if title else False
    screen_asking = any_match(cfg.asking.screen_regex)
    screen_thinking = any_match(cfg.thinking.screen_regex)
    screen_idle = any_match(cfg.idle.screen_regex)

    if title_thinking:
        return "thinking"
    if screen_asking:
        return "asking"
    if title_idle:
        return "idle"
    if screen_thinking:
        return "thinking"
    if screen_idle:
        return "idle"
    return "unknown"


# -------- chrome filter ------------------------------------------------------

_SPINNER_CHARS = "⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟"
_BOX_CHARS = "─│╭╮╰╯┌┐└┘├┤┬┴┼━┃┏┓┗┛╌╎"

_EXTRA_CHROME = [
    re.compile(rf"^\s*[{_BOX_CHARS}]+\s*$"),
    re.compile(rf"^\s*[{_SPINNER_CHARS}]\s"),
    re.compile(r"Usage\s+[░▓█▒]+"),
    re.compile(r"^\s*│.*│\s*$"),
    re.compile(r"^\s*[▗▖▘▝]\s*[▗▖▘▝]"),
    # ASCII-art splash screens built from unicode braille (Kiro's "KIRO"
    # banner, some agents' startup logos). Any row containing 10+ braille
    # glyphs in a run is structural, not reply text.
    re.compile(r"[⠀-⣿]{10,}"),
]


def filter_chrome(lines: list[str], cfg: AgentConfig) -> list[str]:
    pats = cfg.chrome + _EXTRA_CHROME
    kept: list[str] = []
    for ln in lines:
        s = ln.rstrip()
        if not s:
            if kept and kept[-1]:
                kept.append("")
            continue
        if any(p.search(s) for p in pats):
            continue
        kept.append(s)
    while kept and not kept[-1]:
        kept.pop()
    return kept


# -------- pyte helpers -------------------------------------------------------

def _row_to_str(row: dict, cols: int) -> str:
    out = []
    for col in range(cols):
        ch = row.get(col)
        out.append(ch.data if ch and ch.data else " ")
    return "".join(out).rstrip()


def snapshot_lines(screen: "pyte.HistoryScreen", cols: int) -> list[str]:
    """Full known text: history-above + current visible rows."""
    lines = [_row_to_str(row, cols) for row in screen.history.top]
    lines.extend(screen.display)
    while lines and not lines[-1].rstrip():
        lines.pop()
    return lines


def history_lines(screen: "pyte.HistoryScreen", cols: int) -> list[str]:
    """Lines that have scrolled into scrollback (monotonically grows)."""
    return [_row_to_str(row, cols) for row in screen.history.top]


def display_lines(screen: "pyte.HistoryScreen") -> list[str]:
    """Current visible rows (rstripped)."""
    return [ln.rstrip() for ln in screen.display]


# -------- events -------------------------------------------------------------

def load_events(ndjson_path: Path) -> list[dict]:
    events = []
    for line in ndjson_path.read_text().splitlines():
        if not line.strip():
            continue
        ev = json.loads(line)
        ev["data"] = bytes.fromhex(ev["hex"])
        events.append(ev)
    events.sort(key=lambda e: e["ts_ms"])
    return events


def prompt_text(data: bytes) -> str:
    """Clean up a write payload into a human-readable prompt string."""
    # Strip bracketed-paste markers, OSC, CSI, and control bytes.
    data = re.sub(rb"\x1b\[200~|\x1b\[201~", b"", data)
    data = re.sub(rb"\x1b\][^\x07]*\x07", b"", data)
    data = re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z~]", b"", data)
    data = re.sub(rb"[\x00-\x08\x0b-\x0c\x0e-\x1f\x7f]", b"", data)
    text = data.split(b"\r", 1)[0].split(b"\n", 1)[0]
    return text.decode("utf-8", errors="replace").strip()


# -------- extraction ---------------------------------------------------------

@dataclass
class Turn:
    kind: str  # "reply" | "dialog"
    prompt: str
    state_transition: str
    raw_lines: list[str]
    filtered_lines: list[str] = field(default_factory=list)


def extract_codex_markers(artdir: Path, cfg: AgentConfig, cols: int, rows: int) -> list[Turn]:
    """Codex-specific extractor.

    Codex's title/screen-based state transitions don't fire reliably during
    its streamed paints, but its replies are consistently marked `• ` in the
    rendered output. The tricky part is pairing each reply with its prompt:
    the prompt `›` echo sometimes gets wiped when Codex redraws the input
    area before we can sample it (e.g. codex/chat turn 3).

    We anchor turns to USER SUBMIT events (writes containing `\\r`) — those
    are always in stream.ndjson — then take the `• ` rows that first appeared
    between this submit and the next. Prompt text comes from the write bytes,
    not the screen echo.
    """
    events = load_events(artdir / "stream.ndjson")
    screen = pyte.HistoryScreen(cols, rows, history=8000, ratio=1.0)
    stream = pyte.ByteStream(screen)

    def current_bullets() -> list[tuple[int, str]]:
        """Every `• `-prefixed row currently known to the screen, with its
        source index (history rows first, then display rows). Stable index
        lets us compute "new bullets since last snapshot" across submits."""
        out: list[tuple[int, str]] = []
        for i, row in enumerate(history_lines(screen, cols)):
            if row.lstrip().startswith("• "):
                out.append((i, row))
        hist_n = len(history_lines(screen, cols))
        for i, row in enumerate(display_lines(screen)):
            if row.lstrip().startswith("• "):
                out.append((hist_n + i, row))
        return out

    # Flatten events into a single timeline and track where each user submit
    # sits so we can slice bullets per turn.
    submits: list[dict] = []   # {prompt, bullets_at_submit_time}
    prev_bullet_count = 0

    for ev in events:
        if ev["kind"] == "read":
            stream.feed(ev["data"])
            continue
        if ev["kind"] != "write":
            continue
        data = ev["data"]
        if b"\r" not in data and b"\n" not in data:
            continue
        prompt = prompt_text(data)
        if not prompt or prompt.startswith("/"):
            # Slash commands and empty dialog acks don't get replies.
            continue
        submits.append({
            "prompt": prompt,
            "bullet_count_at_submit": len(current_bullets()),
        })

    # Final replay — everything's already fed, capture the final bullets set.
    all_bullets = current_bullets()

    turns: list[Turn] = []
    for idx, s in enumerate(submits):
        start = s["bullet_count_at_submit"]
        end = submits[idx + 1]["bullet_count_at_submit"] if idx + 1 < len(submits) else len(all_bullets)
        new_bullets = all_bullets[start:end]
        if not new_bullets:
            continue
        # Reply = every row from the first new bullet to just before the
        # NEXT bullet (or end of known content). That sweeps up continuation
        # lines that don't themselves start with `• `.
        reply_lines: list[str] = []
        # Reconstruct the full rendered sequence so we can grab between-bullet
        # continuation rows.
        all_rows = history_lines(screen, cols) + display_lines(screen)
        first_idx = new_bullets[0][0]
        last_idx = new_bullets[-1][0]
        # Scan from first bullet forward, stop at the next bullet beyond our
        # range (which belongs to the next turn) or a `›` echo (next prompt).
        j = first_idx
        while j < len(all_rows):
            row = all_rows[j]
            rs = row.lstrip()
            if j > last_idx and rs.startswith("• "):
                break
            if j > first_idx and rs.startswith("› "):
                break
            reply_lines.append(row)
            j += 1
        turns.append(Turn(
            kind="reply",
            prompt=s["prompt"],
            state_transition="marker",
            raw_lines=reply_lines,
        ))

    for t in turns:
        t.filtered_lines = filter_chrome(t.raw_lines, cfg)
    return turns


def extract(artdir: Path, cfg: AgentConfig, cols: int, rows: int) -> list[Turn]:
    if cfg.id == "codex":
        return extract_codex_markers(artdir, cfg, cols, rows)
    events = load_events(artdir / "stream.ndjson")
    screen = pyte.HistoryScreen(cols, rows, history=8000, ratio=1.0)
    stream = pyte.ByteStream(screen)

    turns: list[Turn] = []
    last_hist_len = 0                  # scrollback cursor (monotonic)
    seen_lines: set[str] = set()       # every non-blank line we've emitted so far
    current_prompt = ""
    pending_prompt = ""
    last_state = "unknown"
    was_thinking_since_last_idle = False
    saw_asking_this_cycle = False
    first_idle_seen = False
    # Debounce: a single idle read can be a screen-redraw blip (Kiro shows
    # this during long streams). Require a few consecutive idle reads before
    # we commit to a "this turn is finished" pending_cut.
    idle_debounce_count = 0
    IDLE_DEBOUNCE = 3
    pending_cut: dict | None = None
    # Record submits as they arrive so we can attribute multi-⏺ batched
    # Claude replies back to each original prompt.
    pending_submits: list[str] = []

    def _compute_delta() -> list[str]:
        new_hist = history_lines(screen, cols)[last_hist_len:]
        display = display_lines(screen)
        delta: list[str] = []
        for row in new_hist:
            key = row.rstrip()
            if not key:
                if delta and delta[-1] != "":
                    delta.append("")
                continue
            if key in seen_lines:
                continue
            delta.append(row)
            seen_lines.add(key)
        this_turn_keys = {row.rstrip() for row in delta if row.rstrip()}
        for row in display:
            key = row.rstrip()
            if not key:
                if delta and delta[-1] != "":
                    delta.append("")
                continue
            if key in this_turn_keys:
                continue
            delta.append(row)
            this_turn_keys.add(key)
            seen_lines.add(key)
        return delta

    def _split_by_reply_marker(delta: list[str], n_prompts: int, prompts: list[str],
                                base_cut: dict) -> list[Turn]:
        """Claude batches multiple queued prompts into one thinking cycle.
        Within the resulting delta, each reply starts with `⏺ `. If we have
        more pending_submits than the one attached to the cut, split the
        delta at `⏺` boundaries and pair each block to the next prompt.
        """
        marker_idxs = [i for i, row in enumerate(delta) if row.lstrip().startswith("⏺ ")]
        if len(marker_idxs) < 2 or n_prompts < 2:
            return [Turn(kind=base_cut["kind"], prompt=base_cut["prompt"],
                         state_transition=base_cut["transition"], raw_lines=delta)]
        # Pair markers to prompts (oldest first). If we have fewer prompts
        # than markers, attribute the extra markers to the newest prompt.
        chunk_count = min(len(marker_idxs), n_prompts)
        boundaries = marker_idxs[:chunk_count] + [len(delta)]
        out: list[Turn] = []
        for i in range(chunk_count):
            sub_prompt = prompts[i] if i < len(prompts) else prompts[-1]
            out.append(Turn(
                kind=base_cut["kind"], prompt=sub_prompt,
                state_transition=base_cut["transition"] + "+split",
                raw_lines=delta[boundaries[i]:boundaries[i + 1]],
            ))
        return out

    def commit_pending():
        nonlocal last_hist_len, pending_cut, pending_submits
        if pending_cut is None:
            return
        delta = _compute_delta()
        last_hist_len = len(history_lines(screen, cols))
        if pending_cut["saw_asking"]:
            pending_cut["kind"] = "dialog"
        if delta:
            prompts = list(pending_submits) if pending_submits else [pending_cut["prompt"]]
            for turn in _split_by_reply_marker(delta, len(prompts), prompts, pending_cut):
                turns.append(turn)
        pending_submits = []
        pending_cut = None

    for ev in events:
        if ev["kind"] == "write":
            data = ev["data"]
            is_submit = b"\r" in data or b"\n" in data
            if is_submit:
                new_prompt = prompt_text(data) or current_prompt
                # Queue the submit. If we have a pending_cut at this moment
                # then this new submit closes a clean turn — flush it with
                # the currently-queued prompts, then start fresh.
                if pending_cut is not None:
                    pending_cut["saw_asking"] = pending_cut["saw_asking"] or saw_asking_this_cycle
                    commit_pending()
                pending_submits.append(new_prompt)
                current_prompt = new_prompt
                if pending_prompt == "":
                    pending_prompt = new_prompt
            continue
        if ev["kind"] != "read":
            continue

        stream.feed(ev["data"])
        state = detect_state(screen.title, list(screen.display), cfg)
        if state == "unknown":
            continue

        if state == "thinking":
            idle_debounce_count = 0
            if last_state != "thinking":
                if pending_cut is not None:
                    pending_cut["saw_asking"] = pending_cut["saw_asking"] or saw_asking_this_cycle
                    commit_pending()
                if pending_prompt == "":
                    pending_prompt = current_prompt
                saw_asking_this_cycle = False
            was_thinking_since_last_idle = True
        elif state == "asking":
            idle_debounce_count = 0
            saw_asking_this_cycle = True
        elif state == "idle":
            if not first_idle_seen:
                first_idle_seen = True
                last_hist_len = len(history_lines(screen, cols))
                for row in history_lines(screen, cols) + display_lines(screen):
                    key = row.rstrip()
                    if key:
                        seen_lines.add(key)
                was_thinking_since_last_idle = False
                saw_asking_this_cycle = False
                pending_prompt = ""
                pending_submits = []
                idle_debounce_count = 0
            elif was_thinking_since_last_idle:
                idle_debounce_count += 1
                if idle_debounce_count >= IDLE_DEBOUNCE and pending_cut is None:
                    # Stable idle — the reply has actually finished streaming.
                    pending_cut = {
                        "kind": "dialog" if saw_asking_this_cycle else "reply",
                        "prompt": pending_prompt or current_prompt,
                        "transition": f"{last_state}→idle",
                        "saw_asking": saw_asking_this_cycle,
                    }
                    was_thinking_since_last_idle = False
                    pending_prompt = ""
        last_state = state

    if pending_cut is not None:
        pending_cut["saw_asking"] = pending_cut["saw_asking"] or saw_asking_this_cycle
        commit_pending()
    # Tail content after the last state transition.
    tail: list[str] = []
    for row in history_lines(screen, cols)[last_hist_len:] + display_lines(screen):
        key = row.rstrip()
        if not key:
            if tail and tail[-1] != "":
                tail.append("")
            continue
        if key in seen_lines:
            continue
        tail.append(row)
        seen_lines.add(key)
    if tail:
        turns.append(Turn(
            kind="reply",
            prompt=current_prompt,
            state_transition="eof",
            raw_lines=tail,
        ))

    for t in turns:
        t.filtered_lines = filter_chrome(t.raw_lines, cfg)
    return turns


# -------- rendering ----------------------------------------------------------

def render_text(turns: list[Turn], agent: str, artdir: Path):
    reply_n = sum(1 for t in turns if t.kind == "reply")
    dialog_n = sum(1 for t in turns if t.kind == "dialog")
    print(f"# {agent}:{artdir.name}  ({reply_n} reply, {dialog_n} dialog)")
    for i, t in enumerate(turns):
        prompt = t.prompt or "(no prior prompt)"
        print(f"\n{'=' * 70}")
        print(f"TURN {i}  [{t.kind}] [{t.state_transition}]  prompt: {prompt!r}")
        print(f"  raw={len(t.raw_lines)}  filtered={len(t.filtered_lines)}")
        print("-" * 70)
        for line in t.filtered_lines:
            print(line)


def render_json(turns: list[Turn], agent: str, artdir: Path):
    print(json.dumps({
        "agent": agent,
        "artifact": str(artdir),
        "turns": [
            {
                "index": i,
                "kind": t.kind,
                "prompt": t.prompt,
                "transition": t.state_transition,
                "text": "\n".join(t.filtered_lines),
            }
            for i, t in enumerate(turns)
        ],
    }, ensure_ascii=False, indent=2))


# -------- main ---------------------------------------------------------------

def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("artifact_dir", type=Path)
    ap.add_argument("--agent", help="claude|kiro|codex (inferred from path if omitted)")
    ap.add_argument("--cols", type=int, default=140)
    ap.add_argument("--rows", type=int, default=40)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args(argv)

    artdir: Path = args.artifact_dir
    if not (artdir / "stream.ndjson").exists():
        print(f"no stream.ndjson in {artdir}", file=sys.stderr)
        return 2

    agents = load_agents()
    agent_id = args.agent
    if not agent_id:
        parts = artdir.resolve().parts
        agent_id = parts[-2] if len(parts) >= 2 else ""
    if agent_id not in agents:
        print(f"unknown agent {agent_id!r}; pick one of {list(agents)}", file=sys.stderr)
        return 2
    cfg = agents[agent_id]

    turns = extract(artdir, cfg, args.cols, args.rows)
    if args.json:
        render_json(turns, agent_id, artdir)
    else:
        render_text(turns, agent_id, artdir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
