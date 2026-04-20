#!/bin/bash
# ChatTerm hook — sends JSON messages via FIFO named pipe
# Works with: Claude Code hooks, Kiro CLI hooks, Codex notify
PIPE="$HOME/.chatterm/hook.pipe"
[ -p "$PIPE" ] || exit 0

# Session ID from env or fallback
SID="${CHATTERM_SESSION_ID:-unknown}"

LOG="$HOME/.chatterm/hook.log"
CWD_VAL=""
send() {
  local msg=$(printf '{"session_id":"%s","type":"%s","body":"%s","cwd":"%s"}' "$SID" "$1" "$2" "$CWD_VAL")
  echo "$(date +%H:%M:%S) $msg" >> "$LOG"
  printf '%s\n' "$msg" > "$PIPE" 2>/dev/null
}

EVENT=$(cat)

TYPE=$(echo "$EVENT" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('hook_event_name',d.get('type','')))" 2>/dev/null)
TOOL=$(echo "$EVENT" | python3 -c "import sys,json;print(json.load(sys.stdin).get('tool_name',''))" 2>/dev/null)
CWD_VAL=$(echo "$EVENT" | python3 -c "import sys,json;print(json.load(sys.stdin).get('cwd',''))" 2>/dev/null)
MSG=$(echo "$EVENT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
m=d.get('last_assistant_message','')
if m:
    lines=[l.strip() for l in m.strip().splitlines() if l.strip()]
    print(lines[-1][:120] if lines else '')
elif d.get('message',''): print(d['message'][:120])
elif d.get('output',''): print(str(d['output'])[:120])
else: print('')
" 2>/dev/null)

case "$TYPE" in
  Stop|stop)      [ -n "$MSG" ] && send "reply" "$MSG" || send "done" "Session complete" ;;
  PreToolUse|preToolUse)  send "tool" "▶ ${TOOL}" ;;
  PostToolUse|postToolUse) send "tool" "✓ ${TOOL}" ;;
  Notification)   send "ask" "${MSG:-Waiting for input}" ;;
  response)       send "reply" "$MSG" ;;
  tool_call)      send "tool" "▶ ${TOOL}" ;;
  tool_result)    send "tool" "✓ ${TOOL}" ;;
  *)              [ -n "$MSG" ] && send "event" "$MSG" ;;
esac
