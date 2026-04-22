# ChatTerm agent hooks installer for Windows
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\setup-hooks.ps1
#
# Writes a hook at %APPDATA%\chatterm\hook.py and wires it into:
#   - Claude Code (~/.claude/settings.json)
#   - Kiro CLI    (~/.kiro/agents/chatterm.json)
#   - Codex       (~/.codex/hooks.json)

$ErrorActionPreference = "Stop"

$HookDir = "$env:APPDATA\chatterm"
$Hook = "$HookDir\hook.py"
New-Item -ItemType Directory -Force -Path $HookDir | Out-Null

# --- Install the hook script ---
@'
#!/usr/bin/env python3
"""ChatTerm hook (Windows) - reads JSON on stdin, relays via Named Pipe."""
import os, sys, json, datetime

APPDATA = os.environ.get("APPDATA", os.path.expanduser("~"))
PIPE = r"\\.\pipe\chatterm-hook"
LOG  = os.path.join(APPDATA, "chatterm", "hook.log")
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
    msg = lines[-1][:120] if lines else ""

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
        with open(PIPE, "w") as f:
            f.write(payload + "\n")
    except OSError:
        return  # no reader / pipe missing

mapping = {
    "Stop":         ("reply", msg) if msg else ("done",  "Session complete"),
    "stop":         ("reply", msg) if msg else ("done",  "Session complete"),
    "PreToolUse":   ("tool",  f">> {tool}"),
    "preToolUse":   ("tool",  f">> {tool}"),
    "PostToolUse":  ("tool",  f"ok {tool}"),
    "postToolUse":  ("tool",  f"ok {tool}"),
    "Notification": ("ask",   msg or "Waiting for input"),
    "response":     ("reply", msg),
    "tool_call":    ("tool",  f">> {tool}"),
    "tool_result":  ("tool",  f"ok {tool}"),
}
if ev in mapping:
    kind, body = mapping[ev]
    emit(kind, body)
elif msg:
    emit("event", msg)
'@ | Set-Content -Path $Hook -Encoding UTF8
Write-Host "Hook installed: $Hook"

# --- Claude Code ---
function Setup-Claude {
    $dir = "$env:USERPROFILE\.claude"
    $file = "$dir\settings.json"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    python -c @"
import json, os
f=r'$file'
h=r'$Hook'
s = json.load(open(f)) if os.path.exists(f) else {}
hook = {'type': 'command', 'command': f'python "{h}"'}
changed = False
for ev in ['Stop', 'PreToolUse', 'PostToolUse', 'Notification']:
    rules = s.setdefault('hooks', {}).setdefault(ev, [])
    new_rules = [r for r in rules if 'chatterm' not in str(r).lower()]
    if new_rules != rules:
        rules[:] = new_rules
        changed = True
    if not any(h in str(r) for r in rules):
        rules.append({'matcher': '', 'hooks': [hook]})
        changed = True
if changed:
    json.dump(s, open(f, 'w'), indent=2)
    print('  Claude Code hooks configured')
else:
    print('  Claude Code hooks already configured')
"@
}

# --- Kiro CLI ---
function Setup-Kiro {
    $dir = "$env:USERPROFILE\.kiro\agents"
    $file = "$dir\chatterm.json"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    if ((Test-Path $file) -and (Select-String -Path $file -Pattern "chatterm" -Quiet)) {
        Write-Host "  Kiro CLI hooks already configured"
        return
    }
    $hookCmd = "python `"$Hook`""
    @"
{
  "name": "chatterm",
  "description": "Default agent with ChatTerm notification hooks",
  "tools": ["*"],
  "allowedTools": ["*"],
  "includeMcpJson": true,
  "hooks": {
    "stop": [{"command": "$hookCmd"}],
    "preToolUse": [{"command": "$hookCmd"}],
    "postToolUse": [{"command": "$hookCmd"}]
  }
}
"@ | Set-Content -Path $file -Encoding UTF8
    Write-Host "  Kiro CLI hooks configured (agent: chatterm)"
}

# --- Codex ---
function Setup-Codex {
    $dir = "$env:USERPROFILE\.codex"
    $hooksFile = "$dir\hooks.json"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    python -c @"
import os, json
path=r'$hooksFile'
hook=r'$Hook'
try:
    data = json.load(open(path))
except:
    data = {}
hooks = data.setdefault('hooks', {})
desired = {
    'Stop':       {'matcher': None,   'command': f'python "{hook}"'},
    'PreToolUse': {'matcher': 'Bash', 'command': f'python "{hook}"'},
    'PostToolUse':{'matcher': 'Bash', 'command': f'python "{hook}"'},
}
for event, spec in desired.items():
    rules = hooks.setdefault(event, [])
    rules[:] = [r for r in rules if 'chatterm' not in json.dumps(r).lower()]
    rule = {'hooks': [{'type': 'command', 'command': spec['command']}]}
    if spec['matcher']:
        rule['matcher'] = spec['matcher']
    rules.append(rule)
json.dump(data, open(path, 'w'), indent=2)
print('  Codex hooks configured')
"@
}

Write-Host ""
Write-Host "Configuring agents..."
Setup-Claude
Setup-Kiro
Setup-Codex

Write-Host ""
Write-Host "Done! Restart agents to apply."
