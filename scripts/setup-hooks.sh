#!/bin/bash
# ChatTerm agent hooks installer (self-contained)
#
# Works from:
#   - Repo checkout:  bash scripts/setup-hooks.sh
#   - Installed app:  bash /Applications/ChatTerm.app/Contents/Resources/setup-hooks.sh
#   - Remote:         curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash
#
# Writes a stable hook at ~/.chatterm/hook.sh and wires it into:
#   - Claude Code (~/.claude/settings.json)    — applies globally
#   - Kiro CLI    (~/.kiro/agents/chatterm.json) — requires `--agent chatterm`
#   - Codex       (~/.codex/hooks.json)        — applies globally
set -e

HOOK_DIR="$HOME/.chatterm"
HOOK="$HOOK_DIR/hook.sh"
mkdir -p "$HOOK_DIR"

# --- Install the hook script at a stable path ---
# The script is Python (with shebang) so we can parse and emit JSON safely
# and write the FIFO non-blocking — never hangs the caller.
cat > "$HOOK" << 'CHATTERM_HOOK_EOF'
#!/usr/bin/env python3
"""ChatTerm hook — reads JSON event on stdin, relays via FIFO.

Safe by design:
- Never blocks the caller: FIFO write uses O_NONBLOCK, so a missing or
  stuck reader drops the message instead of hanging the agent.
- JSON is emitted via json.dumps(), so body can contain quotes/backslashes/newlines.

Works with Claude Code hooks, Kiro CLI hooks, Codex notify.
"""
import os, sys, json, datetime

HOME = os.path.expanduser("~")
PIPE = os.path.join(HOME, ".chatterm", "hook.pipe")
LOG  = os.path.join(HOME, ".chatterm", "hook.log")
SID  = os.environ.get("CHATTERM_SESSION_ID", "unknown")

try:
    raw = sys.stdin.read()
    d = json.loads(raw) if raw.strip() else {}
except Exception:
    sys.exit(0)

ev   = d.get("hook_event_name") or d.get("type") or ""
tool = d.get("tool_name") or ""
cwd  = d.get("cwd") or ""
msg_raw = (d.get("last_assistant_message")
           or d.get("message")
           or d.get("output")
           or "")
if not isinstance(msg_raw, str):
    msg_raw = str(msg_raw)
msg = ""
if msg_raw:
    lines = [l.strip() for l in msg_raw.strip().splitlines() if l.strip()]
    # Pick the last narrative line, not a markdown artifact. Skip:
    #   - whole fenced code blocks (```…``` and ~~~…~~~)
    #   - horizontal rules (--- / === / *** / ___)
    #   - bare bullet markers (- / * / • / › / >)
    # If the whole message is code, fall back to the raw last line.
    def _is_md_artifact(l):
        if l and set(l) <= set("-_=*"):
            return True
        if l in {"-", "*", "•", "—", "›", ">"}:
            return True
        return False
    content = []
    inside_fence = False
    for l in lines:
        if l.startswith("```") or l.startswith("~~~"):
            inside_fence = not inside_fence
            continue
        if inside_fence or _is_md_artifact(l):
            continue
        content.append(l)
    pick = content[-1] if content else (lines[-1] if lines else "")
    msg = pick[:120]

def emit(kind, body):
    payload = json.dumps({"session_id": SID, "type": kind, "body": body, "cwd": cwd},
                         ensure_ascii=False)
    ts = datetime.datetime.now().strftime("%H:%M:%S")
    try:
        with open(LOG, "a") as f:
            f.write(f"{ts} {payload}\n")
    except OSError:
        pass
    try:
        fd = os.open(PIPE, os.O_WRONLY | os.O_NONBLOCK)
    except OSError:
        return  # no reader / pipe missing — drop silently
    try:
        os.write(fd, (payload + "\n").encode("utf-8"))
    except OSError:
        pass
    finally:
        os.close(fd)

mapping = {
    "Stop":         ("reply", msg) if msg else ("done",  "Session complete"),
    "stop":         ("reply", msg) if msg else ("done",  "Session complete"),
    "PreToolUse":   ("tool",  f"▶ {tool}"),
    "preToolUse":   ("tool",  f"▶ {tool}"),
    "PostToolUse":  ("tool",  f"✓ {tool}"),
    "postToolUse":  ("tool",  f"✓ {tool}"),
    "Notification": ("ask",   msg or "Waiting for input"),
    "response":     ("reply", msg),
    "tool_call":    ("tool",  f"▶ {tool}"),
    "tool_result":  ("tool",  f"✓ {tool}"),
}
if ev in mapping:
    kind, body = mapping[ev]
    emit(kind, body)
elif msg:
    emit("event", msg)
CHATTERM_HOOK_EOF
chmod +x "$HOOK"
echo "Hook installed: $HOOK"

# --- Claude Code: ~/.claude/settings.json ---
setup_claude() {
  local file="$HOME/.claude/settings.json"
  mkdir -p "$HOME/.claude"
  python3 -c "
import json, os
f='$file'
s = json.load(open(f)) if os.path.exists(f) else {}
hook = {'type': 'command', 'command': '$HOOK'}
changed = False
for ev in ['Stop', 'PreToolUse', 'PostToolUse', 'Notification']:
    rules = s.setdefault('hooks', {}).setdefault(ev, [])
    # Strip any prior chatterm hook entries (stale paths, renames)
    new_rules = [r for r in rules if 'chatterm' not in str(r).lower()]
    if new_rules != rules:
        rules[:] = new_rules
        changed = True
    if not any('$HOOK' in str(r) for r in rules):
        rules.append({'matcher': '', 'hooks': [hook]})
        changed = True
if changed:
    json.dump(s, open(f, 'w'), indent=2)
    print('✅ Claude Code hooks configured')
else:
    print('✅ Claude Code hooks already configured')
"
}

# --- Kiro CLI: both per-agent (~/.kiro/agents/chatterm.json) and global
#     (~/.kiro/settings.json). Per-agent hooks fire only with `--agent chatterm`;
#     global hooks fire for every kiro-cli chat regardless of agent. We install
#     both so default `kiro-cli chat` works out of the box and users who pick
#     `chatterm` as agent also get coverage. ---
setup_kiro() {
  local agent_dir="$HOME/.kiro/agents"
  local agent_file="$agent_dir/chatterm.json"
  local settings_file="$HOME/.kiro/settings.json"
  mkdir -p "$agent_dir"

  if [ ! -f "$agent_file" ] || ! grep -q "$HOOK" "$agent_file"; then
    cat > "$agent_file" << EOF
{
  "name": "chatterm",
  "description": "Default agent with ChatTerm notification hooks",
  "tools": ["*"],
  "allowedTools": ["*"],
  "includeMcpJson": true,
  "hooks": {
    "stop": [{"command": "$HOOK"}],
    "preToolUse": [{"command": "$HOOK"}],
    "postToolUse": [{"command": "$HOOK"}]
  }
}
EOF
    echo "✅ Kiro CLI per-agent hooks configured ($agent_file)"
  else
    echo "✅ Kiro CLI per-agent hooks already configured"
  fi

  # Global hooks: merge into settings.json. Strips stale chatterm entries
  # (prior-repo paths like /Users/.../my_project/chatterm/scripts/...) and
  # inserts the canonical HOOK path under the 4 Claude-compatible event names.
  python3 -c "
import json, os
f = '$settings_file'
s = json.load(open(f)) if os.path.exists(f) else {}
hook_cmd = {'type': 'command', 'command': '$HOOK'}
changed = False
for ev in ['Stop', 'PreToolUse', 'PostToolUse', 'Notification']:
    rules = s.setdefault('hooks', {}).setdefault(ev, [])
    # Drop stale chatterm entries (any command containing 'chatterm' that
    # isn't our canonical HOOK path).
    def drop_stale(rule):
        for h in rule.get('hooks', []):
            c = h.get('command', '')
            if 'chatterm' in c.lower() and c != '$HOOK':
                return True
        return False
    filtered = [r for r in rules if not drop_stale(r)]
    if filtered != rules:
        rules[:] = filtered
        changed = True
    # Inject canonical hook if not already present.
    if not any('$HOOK' in str(r) for r in rules):
        rules.append({'matcher': '', 'hooks': [hook_cmd]})
        changed = True
if changed:
    json.dump(s, open(f, 'w'), indent=2)
    print('✅ Kiro CLI global hooks configured ($settings_file)')
else:
    print('✅ Kiro CLI global hooks already configured')
"
}

# --- Codex: ~/.codex/hooks.json + [features] codex_hooks = true ---
setup_codex() {
  local hooks_file="$HOME/.codex/hooks.json"
  local config_file="$HOME/.codex/config.toml"
  mkdir -p "$HOME/.codex"

  # Enable hooks feature flag — ensure `codex_hooks = true` is present in the
  # [features] block. Preserves other keys in that block and any other sections.
  # Strips stale `notify = ...` lines left by old setups.
  python3 - "$config_file" <<'PYEOF'
import os, sys, re

path = sys.argv[1]
lines = []
if os.path.exists(path):
    with open(path) as f:
        lines = f.read().splitlines(keepends=True)

# Drop stale top-level `notify = ...`
lines = [l for l in lines if not re.match(r'^\s*notify\s*=', l)]

def find_section(lines, name):
    start = end = None
    for i, l in enumerate(lines):
        if re.match(rf'^\s*\[{re.escape(name)}\]\s*$', l):
            start = i + 1
            for j in range(start, len(lines)):
                if re.match(r'^\s*\[', lines[j]):
                    end = j
                    break
            if end is None:
                end = len(lines)
            return start, end
    return None, None

s, e = find_section(lines, 'features')
if s is None:
    # Append a new [features] block
    if lines and not lines[-1].endswith('\n'):
        lines[-1] += '\n'
    if lines and lines[-1].strip():
        lines.append('\n')
    lines.append('[features]\n')
    lines.append('codex_hooks = true\n')
else:
    block = lines[s:e]
    # Drop stale codex_hooks = <anything>
    block = [l for l in block if not re.match(r'^\s*codex_hooks\s*=', l)]
    # Trim trailing blank lines so we don't leave gaps
    while block and not block[-1].strip():
        block.pop()
    block.append('codex_hooks = true\n')
    # Re-add blank separator before the next section, if any
    if e < len(lines):
        block.append('\n')
    lines[s:e] = block

with open(path, 'w') as f:
    f.writelines(lines)
PYEOF

  # Merge into hooks.json — preserve user's entries, replace/add ChatTerm ones.
  python3 - "$hooks_file" "$HOOK" <<'PYEOF'
import os, sys, json

path, hook = sys.argv[1], sys.argv[2]
try:
    with open(path) as f:
        data = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    data = {}

hooks = data.setdefault('hooks', {})
desired = {
    'Stop':        {'matcher': None,   'command': hook},
    'PreToolUse':  {'matcher': 'Bash', 'command': hook},
    'PostToolUse': {'matcher': 'Bash', 'command': hook},
}

def is_chatterm_entry(entry):
    return 'chatterm' in json.dumps(entry).lower()

for event, spec in desired.items():
    rules = hooks.setdefault(event, [])
    # Strip any prior ChatTerm entry (stale paths etc.)
    rules[:] = [r for r in rules if not is_chatterm_entry(r)]
    rule = {'hooks': [{'type': 'command', 'command': spec['command']}]}
    if spec['matcher']:
        rule['matcher'] = spec['matcher']
    rules.append(rule)

with open(path, 'w') as f:
    json.dump(data, f, indent=2)
PYEOF

  echo "✅ Codex hooks configured"
}

echo ""
echo "Configuring agents..."
setup_claude
setup_kiro
setup_codex

echo ""
echo "Done! Restart agents to apply."
echo "Test FIFO: echo '{\"session_id\":\"s0\",\"type\":\"reply\",\"body\":\"test\"}' > ~/.chatterm/hook.pipe"
