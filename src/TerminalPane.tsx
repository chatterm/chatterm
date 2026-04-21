import { useEffect, useRef, useState } from "react";
import { Session, OutputLine, SessionStatus, statusColor, statusLabel } from "./types";
import { Ic, KindIcon } from "./Icons";
import { Avatar } from "./Sidebar";
import { parseAnsi } from "./ansi";

const colorMap: Record<string, string> = {
  red: "var(--ansi-red)", green: "var(--ansi-green)", yellow: "var(--ansi-yellow)",
  blue: "var(--ansi-blue)", magenta: "var(--ansi-magenta)", cyan: "var(--ansi-cyan)",
  white: "var(--ansi-white)", gray: "var(--ansi-gray)",
  "bright-red": "var(--ansi-bright-red)", "bright-green": "var(--ansi-bright-green)",
  "bright-yellow": "var(--ansi-bright-yellow)", "bright-blue": "var(--ansi-bright-blue)",
};

const base: React.CSSProperties = {
  fontFamily: "'JetBrains Mono', monospace", fontSize: 13, lineHeight: 1.55,
  whiteSpace: "pre-wrap", wordBreak: "break-word", flex: 1, minWidth: 0,
};

function LineContent({ line }: { line: OutputLine }) {
  const color = line.color ? colorMap[line.color] : undefined;

  if (line.t === "cmd") {
    if (line.inline) return <span style={{ ...base, color: "var(--text-strong)", fontWeight: 500 }}>{line.text}</span>;
    return <span style={{ ...base, color: "var(--text-strong)", fontWeight: 500 }}>
      <span style={{ color: "var(--accent-hover)" }}>❯ </span>{line.text}
    </span>;
  }
  if (line.t === "sys") return <span style={{ ...base, color: "var(--text-mute)", fontStyle: "italic" }}>― {line.text} ―</span>;
  if (line.t === "err") return <span style={{ ...base, color: "var(--ansi-bright-red)" }}>{line.text}</span>;
  if (line.t === "agent") return (
    <span style={{ ...base, color: "var(--text)" }}>
      {line.who ? <span style={{ color: line.color || "var(--ansi-magenta)", fontWeight: 600 }}>[{line.who}] </span>
        : <span style={{ color: "var(--ansi-magenta)", fontWeight: 600 }}>● </span>}
      {line.text}
    </span>
  );
  if (line.t === "tool") return (
    <div style={{ ...base, display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}>
      <span style={{ fontSize: 10, padding: "1px 6px", borderRadius: 3, background: "rgba(86,156,214,0.15)", color: "var(--ansi-bright-blue)", fontWeight: 600, letterSpacing: 0.3 }}>TOOL</span>
      <span style={{ color: "var(--ansi-cyan)", fontWeight: 500 }}>{line.tool}</span>
      <span style={{ color: "var(--text-mute)" }}>({line.args})</span>
    </div>
  );
  if (line.t === "diff") return (
    <div style={{ ...base, border: "1px solid var(--border-strong)", borderRadius: 4, margin: "4px 0", overflow: "hidden" }}>
      <div style={{ padding: "4px 10px", background: "#2d2d30", fontSize: 11, color: "var(--text-dim)" }}>{line.text}</div>
      <div style={{ padding: "4px 0" }}>
        {line.diff?.map((d, i) => (
          <div key={i} style={{
            padding: "0 10px",
            background: d.kind === "add" ? "rgba(155,185,85,0.12)" : d.kind === "rem" ? "rgba(244,135,113,0.12)" : "transparent",
            color: d.kind === "add" ? "var(--ansi-bright-green)" : d.kind === "rem" ? "var(--ansi-bright-red)" : "var(--text-dim)",
          }}>{d.text}</div>
        ))}
      </div>
    </div>
  );
  if (line.t === "prog") {
    const done = (line.pct ?? 0) >= 100;
    return (
      <div style={{ ...base, padding: "4px 0" }}>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 3 }}>
          <span style={{ color: "var(--text)" }}>{line.label}</span>
          <span style={{ color: "var(--text-dim)" }}>{line.detail}</span>
        </div>
        <div style={{ height: 4, background: "#2d2d30", borderRadius: 2, overflow: "hidden" }}>
          <div style={{ width: `${line.pct}%`, height: "100%", background: done ? "var(--status-running)" : "var(--accent)", transition: "width 400ms ease-out" }} />
        </div>
      </div>
    );
  }
  if (line.t === "done") {
    const ok = line.success !== false;
    return (
      <div style={{ ...base, padding: "6px 10px", margin: "6px 0",
        background: ok ? "rgba(78,201,176,0.08)" : "rgba(244,135,113,0.08)",
        border: `1px solid ${ok ? "rgba(78,201,176,0.3)" : "rgba(244,135,113,0.3)"}`,
        borderLeft: `3px solid ${ok ? "var(--status-running)" : "var(--status-error)"}`, borderRadius: 4,
      }}>
        <div style={{ color: ok ? "var(--status-running)" : "var(--status-error)", fontWeight: 600 }}>{line.text}</div>
        {line.summary && (
          <div style={{ display: "flex", gap: 14, marginTop: 6, fontSize: 11, color: "var(--text-dim)" }}>
            <span><b style={{ color: "var(--text)" }}>{line.summary.filesChanged}</b> files</span>
            <span style={{ color: "var(--ansi-bright-green)" }}>+{line.summary.insertions}</span>
            <span style={{ color: "var(--ansi-bright-red)" }}>−{line.summary.deletions}</span>
            <span><b style={{ color: "var(--text)" }}>{line.summary.testsPassed}</b> tests passed</span>
          </div>
        )}
      </div>
    );
  }
  if (line.t === "ask") return (
    <div style={{ ...base, padding: "8px 10px", margin: "6px 0", background: "rgba(220,220,170,0.08)", borderLeft: "3px solid var(--ansi-yellow)", borderRadius: 4 }}>
      <div style={{ color: "var(--ansi-yellow)", fontWeight: 600, marginBottom: 4, display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ fontSize: 10, padding: "1px 5px", background: "rgba(220,220,170,0.18)", borderRadius: 3 }}>NEEDS YOU</span>
        {line.text}
      </div>
      <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
        {line.choices?.map((c, i) => (
          <button key={i} style={{
            padding: "4px 10px", fontSize: 12, borderRadius: 3, cursor: "pointer",
            background: i === 0 ? "var(--accent)" : "transparent",
            border: i === 0 ? "1px solid var(--accent)" : "1px solid var(--border-strong)",
            color: i === 0 ? "white" : "var(--text)", fontFamily: "inherit",
          }}>{c}</button>
        ))}
      </div>
    </div>
  );
  return <span style={{ ...base, color: color || "var(--text)" }}>{parseAnsi(line.text) || "\u00a0"}</span>;
}

function TermLine({ line }: { line: OutputLine }) {
  return (
    <div className="term-line" style={{ position: "relative", padding: "0 16px", minHeight: "1.5em", display: "flex", alignItems: "flex-start" }}>
      <LineContent line={line} />
      {line.t !== "sys" && line.t !== "done" && line.t !== "prog" && line.t !== "diff" && line.t !== "ask" && (
        <div className="row-hover-actions" style={{
          position: "absolute", right: 16, top: 0, display: "flex", gap: 2,
          background: "var(--panel-bg)", border: "1px solid var(--border-strong)", borderRadius: 4, padding: 2,
        }}>
          <RowBtn title="React"><Ic.smile /></RowBtn>
          <RowBtn title="Reply"><Ic.reply /></RowBtn>
          <RowBtn title="Forward"><Ic.forward /></RowBtn>
          <RowBtn title="Copy"><Ic.copy /></RowBtn>
        </div>
      )}
    </div>
  );
}

function RowBtn({ children, title, onClick }: { children: React.ReactNode; title: string; onClick?: () => void }) {
  return (
    <button onClick={onClick} title={title} style={{
      background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer", padding: 4, borderRadius: 3, display: "flex",
    }}
      onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; }}
      onMouseLeave={e => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; }}
    >{children}</button>
  );
}

function HeaderBtn({ children, onClick, active, title }: { children: React.ReactNode; onClick: () => void; active?: boolean; title: string }) {
  return (
    <button onClick={onClick} title={title} style={{
      background: active ? "var(--sidebar-active)" : "transparent", border: "none",
      color: active ? "var(--accent-hover)" : "var(--text-dim)", cursor: "pointer", padding: 6, borderRadius: 4, display: "flex",
    }}
      onMouseEnter={e => { if (!active) { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; } }}
      onMouseLeave={e => { if (!active) { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; } }}
    >{children}</button>
  );
}

function StatusPill({ status }: { status: SessionStatus }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5 }}>
      <span style={{ width: 6, height: 6, borderRadius: "50%", background: statusColor(status), display: "inline-block" }}
        className={status === "running" ? "pulse-running" : ""} />
      <span style={{ color: statusColor(status), fontWeight: 500 }}>{statusLabel(status)}</span>
    </span>
  );
}

function Prompt({ session }: { session: Session }) {
  if (session.kind === "ssh") return <span style={{ color: "var(--ansi-bright-green)" }}>{session.user}@{session.host?.split(".")[0]}:{session.cwd}$&nbsp;</span>;
  if (session.kind === "shell") return <span style={{ color: "var(--ansi-bright-green)" }}>{session.cwd || "~"} ❯&nbsp;</span>;
  if (session.kind === "agent" || session.kind === "group") return <span style={{ color: "var(--accent-hover)" }}>@{session.short.split(" ")[0]} ❯&nbsp;</span>;
  return <span style={{ color: "var(--accent-hover)" }}>❯&nbsp;</span>;
}

interface Props {
  session: Session;
  onSend: (text: string) => void;
  onTogglePin: () => void;
  onToggleMute: () => void;
}

export default function TerminalPane({ session, onSend, onTogglePin, onToggleMute }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [input, setInput] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [session.lines.length]);
  useEffect(() => { setInput(""); inputRef.current?.focus(); }, [session.id]);

  const submit = () => { if (!input.trim()) return; onSend(input); setInput(""); };

  const readOnly = session.kind === "ci" || session.kind === "hook";

  return (
    <main style={{ flex: 1, display: "flex", flexDirection: "column", background: "var(--editor-bg)", minWidth: 0 }}>
      {/* Header */}
      <div className="no-select" style={{
        display: "flex", alignItems: "center", gap: 10, padding: "10px 16px",
        borderBottom: "1px solid var(--border)", flex: "0 0 auto",
      }}>
        <Avatar av={session.avatar} size={30} status={session.status} group={session.avatar.group} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-strong)" }}>{session.name}</div>
            <KindIcon kind={session.kind} style={{ color: "var(--text-mute)" }} />
          </div>
          <div className="mono" style={{ fontSize: 11, color: "var(--text-dim)", display: "flex", gap: 10, alignItems: "center", marginTop: 1 }}>
            <StatusPill status={session.status} />
            {session.cwd && <span>{session.cwd}</span>}
            {session.branch && session.branch !== "—" && <span style={{ color: "var(--ansi-cyan)" }}>⎇ {session.branch}</span>}
            {session.model && <span style={{ color: "var(--ansi-magenta)" }}>{session.model}</span>}
            {session.host && <span>{session.user}@{session.host}</span>}
            {session.pid && <span>pid {session.pid}</span>}
          </div>
        </div>
        <HeaderBtn onClick={onTogglePin} active={session.pinned} title={session.pinned ? "Unpin" : "Pin"}><Ic.pin /></HeaderBtn>
        <HeaderBtn onClick={onToggleMute} active={session.muted} title={session.muted ? "Unmute" : "Mute"}><Ic.mute /></HeaderBtn>
        <HeaderBtn onClick={() => {}} active={false} title="Details"><Ic.dots /></HeaderBtn>
      </div>

      {/* Message stream */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "10px 0 20px" }}>
        {session.lines.map((line, i) => <TermLine key={i} line={line} />)}
        {session.status === "running" && (
          <div style={{ padding: "4px 16px" }}>
            <span className="cursor" />
          </div>
        )}
      </div>

      {/* Input bar */}
      <div style={{ flex: "0 0 auto", background: "var(--editor-bg)", borderTop: "1px solid var(--border)" }}>
        <div style={{ padding: "10px 16px", display: "flex", gap: 8, alignItems: "flex-start" }}>
          <div className="mono" style={{ fontSize: 13, paddingTop: 4, whiteSpace: "nowrap" }}>
            <Prompt session={session} />
          </div>
          <textarea
            ref={inputRef}
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); } }}
            disabled={readOnly}
            placeholder={readOnly ? "(read-only log stream)" : session.kind === "agent" ? "Message the agent — Shift+Enter for newline" : "Type a command…"}
            rows={1}
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none", resize: "none",
              color: "var(--text-strong)", fontFamily: "'JetBrains Mono',monospace", fontSize: 13,
              lineHeight: 1.5, padding: "4px 0", minHeight: 22,
            }}
          />
          <span className="mono" style={{ fontSize: 10, color: "var(--text-mute)", padding: "2px 5px", background: "#2d2d30", borderRadius: 3, marginTop: 4 }}>↵ send</span>
        </div>
      </div>
    </main>
  );
}
