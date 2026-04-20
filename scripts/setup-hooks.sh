#!/bin/bash
# Setup ChatTerm hooks for Claude Code, Kiro CLI, and OpenAI Codex
set -e

HOOK="$(cd "$(dirname "$0")" && pwd)/chatterm-hook.sh"
chmod +x "$HOOK"

echo "Setting up ChatTerm hooks..."

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

# --- Kiro CLI: ~/.kiro/agents/chatterm.json ---
setup_kiro() {
  local dir="$HOME/.kiro/agents"
  local file="$dir/chatterm.json"
  mkdir -p "$dir"
  if [ -f "$file" ] && grep -q "$HOOK" "$file"; then
    echo "✅ Kiro CLI hooks already configured"
    return
  fi
  cat > "$file" << EOF
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
  echo "✅ Kiro CLI hooks configured (agent: chatterm)"
  echo "   Switch to it with: /agent swap chatterm"
}

# --- Codex: ~/.codex/hooks.json + feature flag ---
setup_codex() {
  local hooks_file="$HOME/.codex/hooks.json"
  local config_file="$HOME/.codex/config.toml"
  mkdir -p "$HOME/.codex"

  # Enable hooks feature flag + remove stale notify line
  if [ -f "$config_file" ]; then
    python3 -c "
lines = open('$config_file').readlines()
lines = [l for l in lines if not l.strip().startswith('notify =')]
has_features = any(l.strip() == '[features]' for l in lines)
if not has_features:
    insert_at = next((i for i,l in enumerate(lines) if l.strip().startswith('[')), len(lines))
    lines.insert(insert_at, '[features]\ncodex_hooks = true\n\n')
open('$config_file','w').writelines(lines)
"
    fi
  else
    printf '[features]\ncodex_hooks = true\n' > "$config_file"
  fi

  # Create hooks.json
  if [ -f "$hooks_file" ] && grep -q "$HOOK" "$hooks_file"; then
    echo "✅ Codex hooks already configured"
    return
  fi
  cat > "$hooks_file" << EOF
{
  "hooks": {
    "Stop": [{"hooks": [{"type": "command", "command": "$HOOK"}]}],
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "$HOOK"}]}],
    "PostToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "$HOOK"}]}]
  }
}
EOF
  echo "✅ Codex hooks configured"
}

setup_claude
setup_kiro
setup_codex

echo ""
echo "Done! Restart agents to apply."
echo "Test FIFO: echo '{\"session_id\":\"s0\",\"type\":\"reply\",\"body\":\"test\"}' > ~/.chatterm/hook.pipe"
