// Terminal pane: the main area. Pure output stream with header + input bar.

const colorMap = {
  red: "var(--ansi-red)", green: "var(--ansi-green)", yellow: "var(--ansi-yellow)",
  blue: "var(--ansi-blue)", magenta: "var(--ansi-magenta)", cyan: "var(--ansi-cyan)",
  white: "var(--ansi-white)", gray: "var(--ansi-gray)",
  "bright-red": "var(--ansi-bright-red)", "bright-green": "var(--ansi-bright-green)",
  "bright-yellow": "var(--ansi-bright-yellow)", "bright-blue": "var(--ansi-bright-blue)",
  "bright-magenta": "var(--ansi-bright-magenta)", "bright-cyan": "var(--ansi-bright-cyan)",
};

const TermHeader = ({ session, onTogglePin, onToggleMute, onOpenDetails, detailsOpen }) => {
  return (
    <div className="no-select" style={{
      display: "flex", alignItems: "center", gap: 10, padding: "10px 16px",
      borderBottom: "1px solid var(--border)", background: "var(--editor-bg)",
      flex: "0 0 auto",
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
          {session.port && <span>:{session.port}</span>}
          {session.pid && <span>pid {session.pid}</span>}
        </div>
      </div>
      <HeaderBtn onClick={onTogglePin} active={session.pinned} title={session.pinned ? "Unpin" : "Pin"}><Ic.pin /></HeaderBtn>
      <HeaderBtn onClick={onToggleMute} active={session.muted} title={session.muted ? "Unmute" : "Mute"}><Ic.mute /></HeaderBtn>
      <HeaderBtn onClick={onOpenDetails} active={detailsOpen} title="Details"><Ic.dots /></HeaderBtn>
    </div>
  );
};

const HeaderBtn = ({ children, onClick, active, title }) => (
  <button onClick={onClick} title={title} style={{
    background: active ? "var(--sidebar-active)" : "transparent", border: "none",
    color: active ? "var(--accent-hover)" : "var(--text-dim)", cursor: "pointer",
    padding: 6, borderRadius: 4, display: "flex",
  }}
    onMouseEnter={(e) => { if (!active) { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; } }}
    onMouseLeave={(e) => { if (!active) { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; } }}
  >{children}</button>
);

const StatusPill = ({ status }) => (
  <span style={{ display: "inline-flex", alignItems: "center", gap: 5 }}>
    <span style={{
      width: 6, height: 6, borderRadius: "50%", background: statusColor(status),
      display: "inline-block",
    }} className={status === "running" ? "pulse-running" : ""} />
    <span style={{ color: statusColor(status), fontWeight: 500 }}>{statusLabel(status)}</span>
  </span>
);

// Quote banner shown above input if replying to a line
const QuoteBanner = ({ quote, onClear }) => {
  if (!quote) return null;
  return (
    <div className="mono" style={{
      padding: "6px 16px", background: "rgba(14,99,156,0.10)",
      borderLeft: "2px solid var(--accent)",
      fontSize: 11, color: "var(--text-dim)", display: "flex", alignItems: "center", gap: 8,
    }}>
      <Ic.reply style={{ color: "var(--accent)" }} />
      <span style={{ color: "var(--text-mute)" }}>Replying to:</span>
      <span style={{ flex: 1, color: "var(--text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {quote.text}
      </span>
      <button onClick={onClear} style={{
        background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer", padding: 2, display: "flex",
      }}><Ic.x /></button>
    </div>
  );
};

// A single terminal line with hover actions for reactions / reply / forward
const TermLine = ({ line, idx, sessionId, reactions, onReact, onQuote, onForward }) => {
  const key = `${sessionId}:${idx}`;
  const rx = reactions[key] || {};
  const [showRxPicker, setShowRxPicker] = React.useState(false);

  return (
    <div className="term-line" style={{
      position: "relative", padding: "0 16px", minHeight: "1.5em",
      display: "flex", alignItems: "flex-start", gap: 0,
    }}>
      <LineContent line={line} />
      {line.t !== "sys" && line.t !== "done" && line.t !== "prog" && line.t !== "diff" && line.t !== "ask" && (
        <div className="row-hover-actions" style={{
          position: "absolute", right: 16, top: 0, display: "flex", gap: 2,
          background: "var(--panel-bg)", border: "1px solid var(--border-strong)", borderRadius: 4,
          padding: 2,
        }}>
          <RowBtn onClick={() => setShowRxPicker(!showRxPicker)} title="React"><Ic.smile /></RowBtn>
          <RowBtn onClick={() => onQuote(line, idx)} title="Quote reply"><Ic.reply /></RowBtn>
          <RowBtn onClick={() => onForward(line, idx)} title="Forward to another agent"><Ic.forward /></RowBtn>
          <RowBtn title="Copy"><Ic.copy /></RowBtn>
          {showRxPicker && (
            <div style={{
              position: "absolute", right: 0, top: "calc(100% + 4px)",
              background: "#2d2d30", border: "1px solid var(--border-strong)", borderRadius: 6,
              padding: 4, display: "flex", gap: 2, zIndex: 10,
              boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
            }}>
              {["👍", "🎉", "🐛", "🔥", "👀", "🤔"].map((e) => (
                <button key={e} onClick={() => { onReact(key, e); setShowRxPicker(false); }} style={{
                  background: "transparent", border: "none", cursor: "pointer",
                  padding: "4px 6px", borderRadius: 4, fontSize: 15,
                }}
                  onMouseEnter={(ev) => ev.currentTarget.style.background = "var(--sidebar-active)"}
                  onMouseLeave={(ev) => ev.currentTarget.style.background = "transparent"}
                >{e}</button>
              ))}
            </div>
          )}
        </div>
      )}
      {Object.keys(rx).length > 0 && (
        <div style={{ position: "absolute", left: 16, bottom: -22, display: "flex", gap: 3, zIndex: 1 }}>
          {Object.entries(rx).map(([emoji, count]) => (
            <div key={emoji} className="reaction-chip active" onClick={() => onReact(key, emoji)}>
              <span>{emoji}</span><span style={{ fontVariantNumeric: "tabular-nums" }}>{count}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

const RowBtn = ({ children, onClick, title }) => (
  <button onClick={onClick} title={title} style={{
    background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer",
    padding: 4, borderRadius: 3, display: "flex",
  }}
    onMouseEnter={(e) => { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; }}
    onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; }}
  >{children}</button>
);

const LineContent = ({ line }) => {
  const baseStyle = {
    fontFamily: "'JetBrains Mono', ui-monospace, monospace",
    fontSize: 13, lineHeight: 1.55, whiteSpace: "pre-wrap", wordBreak: "break-word",
    flex: 1, minWidth: 0,
  };
  const color = line.color ? colorMap[line.color] : undefined;

  if (line.t === "cmd") {
    if (line.inline) {
      return <span style={{ ...baseStyle, color: "var(--text-strong)", fontWeight: 500 }}>{line.text}</span>;
    }
    return (
      <span style={{ ...baseStyle, color: "var(--text-strong)", fontWeight: 500 }}>
        <span style={{ color: "var(--accent-hover)" }}>❯ </span>{line.text}
      </span>
    );
  }

  if (line.t === "sys") {
    return <span style={{ ...baseStyle, color: "var(--text-mute)", fontStyle: "italic" }}>― {line.text} ―</span>;
  }

  if (line.t === "agent") {
    return (
      <span style={{ ...baseStyle, color: "var(--text)" }}>
        {line.who && (
          <span style={{ color: line.color || "var(--ansi-magenta)", fontWeight: 600 }}>
            [{line.who}]{" "}
          </span>
        )}
        {!line.who && <span style={{ color: "var(--ansi-magenta)", fontWeight: 600 }}>● </span>}
        {line.text}
      </span>
    );
  }

  if (line.t === "tool") {
    return (
      <div style={{ ...baseStyle, display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}>
        <span style={{
          fontSize: 10, padding: "1px 6px", borderRadius: 3,
          background: "rgba(86,156,214,0.15)", color: "var(--ansi-bright-blue)",
          fontWeight: 600, letterSpacing: 0.3,
        }}>TOOL</span>
        <span style={{ color: "var(--ansi-cyan)", fontWeight: 500 }}>{line.tool}</span>
        <span style={{ color: "var(--text-mute)" }}>({line.args})</span>
      </div>
    );
  }

  if (line.t === "diff") {
    return (
      <div style={{ ...baseStyle, border: "1px solid var(--border-strong)", borderRadius: 4, margin: "4px 0", overflow: "hidden" }}>
        <div style={{ padding: "4px 10px", background: "#2d2d30", fontSize: 11, color: "var(--text-dim)" }}>{line.text}</div>
        <div style={{ padding: "4px 0" }}>
          {line.diff.map((d, i) => (
            <div key={i} style={{
              padding: "0 10px",
              background: d.kind === "add" ? "rgba(155,185,85,0.12)" : d.kind === "rem" ? "rgba(244,135,113,0.12)" : "transparent",
              color: d.kind === "add" ? "var(--ansi-bright-green)" : d.kind === "rem" ? "var(--ansi-bright-red)" : "var(--text-dim)",
            }}>{d.text}</div>
          ))}
        </div>
      </div>
    );
  }

  if (line.t === "prog") {
    const pct = line.pct;
    const done = pct >= 100;
    return (
      <div style={{ ...baseStyle, padding: "4px 0" }}>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 3 }}>
          <span style={{ color: "var(--text)" }}>{line.label}</span>
          <span style={{ color: "var(--text-dim)" }}>{line.detail}</span>
        </div>
        <div style={{ height: 4, background: "#2d2d30", borderRadius: 2, overflow: "hidden" }}>
          <div style={{
            width: `${pct}%`, height: "100%",
            background: done ? "var(--status-running)" : "var(--accent)",
            transition: "width 400ms ease-out",
          }} />
        </div>
      </div>
    );
  }

  if (line.t === "done") {
    const ok = line.success !== false;
    return (
      <div style={{ ...baseStyle, padding: "6px 10px", margin: "6px 0",
        background: ok ? "rgba(78,201,176,0.08)" : "rgba(244,135,113,0.08)",
        border: `1px solid ${ok ? "rgba(78,201,176,0.3)" : "rgba(244,135,113,0.3)"}`,
        borderLeft: `3px solid ${ok ? "var(--status-running)" : "var(--status-error)"}`,
        borderRadius: 4,
      }}>
        <div style={{ color: ok ? "var(--status-running)" : "var(--status-error)", fontWeight: 600 }}>
          {line.text}
        </div>
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

  if (line.t === "ask") {
    return (
      <div style={{ ...baseStyle, padding: "8px 10px", margin: "6px 0",
        background: "rgba(220,220,170,0.08)",
        borderLeft: "3px solid var(--status-waiting)",
        borderRadius: 4,
      }}>
        <div style={{ color: "var(--status-waiting)", fontWeight: 600, marginBottom: 4, display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ fontSize: 10, padding: "1px 5px", background: "rgba(220,220,170,0.18)", borderRadius: 3 }}>NEEDS YOU</span>
          {line.text}
        </div>
        <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
          {line.choices.map((c, i) => (
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
  }

  if (line.t === "err") {
    return <span style={{ ...baseStyle, color: "var(--ansi-bright-red)" }}>{line.text}</span>;
  }

  // default: out
  return (
    <span style={{ ...baseStyle, color: color || "var(--text)" }}>
      {line.text || "\u00a0"}
    </span>
  );
};

const Prompt = ({ session }) => {
  if (session.kind === "ssh") {
    return <span style={{ color: "var(--ansi-bright-green)" }}>{session.user}@{session.host.split(".")[0]}:{session.cwd}$&nbsp;</span>;
  }
  if (session.kind === "shell") {
    return <span style={{ color: "var(--ansi-bright-green)" }}>{session.cwd} ❯&nbsp;</span>;
  }
  if (session.kind === "agent" || session.kind === "group") {
    return <span style={{ color: "var(--accent-hover)" }}>@{session.short.split(" ")[0]} ❯&nbsp;</span>;
  }
  return <span style={{ color: "var(--accent-hover)" }}>❯&nbsp;</span>;
};

const InputBar = ({ session, quote, onClearQuote, onSend }) => {
  const [val, setVal] = React.useState("");
  const ref = React.useRef(null);

  React.useEffect(() => { ref.current?.focus(); }, [session.id]);

  const submit = () => {
    if (!val.trim()) return;
    onSend(val);
    setVal("");
  };

  const placeholder = {
    agent: "Message the agent — Shift+Enter for newline",
    group: "Message the group — use @agent to address one",
    shell: "Type a shell command",
    process: "stdin to process",
    ssh: "Type a shell command on remote host",
    ci: "(read-only log stream)",
    hook: "(read-only log stream)",
  }[session.kind] || "Type a message…";

  const readOnly = session.kind === "ci" || session.kind === "hook";

  return (
    <div style={{ flex: "0 0 auto", background: "var(--editor-bg)", borderTop: "1px solid var(--border)" }}>
      <QuoteBanner quote={quote} onClear={onClearQuote} />
      <div style={{ padding: "10px 16px", display: "flex", gap: 8, alignItems: "flex-start" }}>
        <div className="mono" style={{ fontSize: 13, paddingTop: 4, whiteSpace: "nowrap" }}>
          <Prompt session={session} />
        </div>
        <textarea
          ref={ref}
          value={val}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); }
          }}
          disabled={readOnly}
          placeholder={placeholder}
          rows={1}
          style={{
            flex: 1, background: "transparent", border: "none", outline: "none", resize: "none",
            color: "var(--text-strong)", fontFamily: "'JetBrains Mono', monospace", fontSize: 13,
            lineHeight: 1.5, padding: "4px 0", minHeight: 22,
          }}
        />
        <div style={{ display: "flex", gap: 4, alignItems: "center", paddingTop: 2 }}>
          <span className="mono" style={{ fontSize: 10, color: "var(--text-mute)", padding: "2px 5px", background: "#2d2d30", borderRadius: 3 }}>↵ send</span>
        </div>
      </div>
    </div>
  );
};

const TerminalPane = ({ session, reactions, onReact, onTogglePin, onToggleMute, onOpenDetails, detailsOpen, onSend }) => {
  const scrollRef = React.useRef(null);
  const [quote, setQuote] = React.useState(null);

  React.useEffect(() => {
    setQuote(null);
    // scroll to bottom when session changes
    requestAnimationFrame(() => {
      if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    });
  }, [session.id]);

  React.useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [session.lines.length]);

  return (
    <main style={{ flex: 1, display: "flex", flexDirection: "column", background: "var(--editor-bg)", minWidth: 0 }}>
      <TermHeader session={session} onTogglePin={onTogglePin} onToggleMute={onToggleMute}
        onOpenDetails={onOpenDetails} detailsOpen={detailsOpen} />
      <div ref={scrollRef} style={{
        flex: 1, overflowY: "auto", padding: "10px 0 20px",
      }}>
        {session.lines.map((line, i) => (
          <TermLine key={i} line={line} idx={i} sessionId={session.id}
            reactions={reactions} onReact={onReact}
            onQuote={(l) => setQuote({ ...l, idx: i })}
            onForward={(l) => onSend && onSend(`/forward line #${i} to: `)}
          />
        ))}
        {session.status === "running" && (
          <div style={{ padding: "4px 16px", display: "flex", alignItems: "center", gap: 8 }}>
            <span className="cursor" style={{ width: 7, height: 14 }} />
          </div>
        )}
      </div>
      <InputBar session={session} quote={quote} onClearQuote={() => setQuote(null)} onSend={(t) => { onSend(t, quote); setQuote(null); }} />
    </main>
  );
};

Object.assign(window, { TerminalPane });
