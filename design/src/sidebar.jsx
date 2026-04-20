// Avatar: monogram tile with color + optional status dot and group ring.
const Avatar = ({ av, size = 36, status, kind, group }) => {
  const s = size;
  const font = Math.round(s * 0.4);
  return (
    <div style={{ position: "relative", width: s, height: s, flex: `0 0 ${s}px` }}>
      <div style={{
        width: s, height: s, borderRadius: 6, background: av.color,
        color: "#1e1e1e", display: "flex", alignItems: "center", justifyContent: "center",
        fontFamily: "'JetBrains Mono', monospace", fontWeight: 700, fontSize: font,
        letterSpacing: -0.5, boxShadow: "inset 0 -1px 0 rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.12)",
      }}>{av.mono}</div>
      {group && (
        <div style={{
          position: "absolute", right: -3, bottom: -3, width: 14, height: 14, borderRadius: 4,
          background: "var(--av-4)", border: "2px solid var(--sidebar-bg)",
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 8, color: "#1e1e1e", fontWeight: 700, fontFamily: "'JetBrains Mono',monospace",
        }}>3</div>
      )}
      {status && !group && (
        <div title={status} style={{
          position: "absolute", right: -2, bottom: -2, width: 10, height: 10, borderRadius: "50%",
          background: statusColor(status),
          border: "2px solid var(--sidebar-bg)",
          ...(status === "running" ? {} : {}),
        }} className={status === "running" ? "pulse-running" : ""} />
      )}
    </div>
  );
};

function statusColor(s) {
  return {
    running: "var(--status-running)",
    waiting: "var(--status-waiting)",
    error: "var(--status-error)",
    done: "var(--status-done)",
    idle: "var(--status-idle)",
  }[s] || "var(--status-idle)";
}

function statusLabel(s) {
  return { running: "Running", waiting: "Awaiting you", error: "Error", done: "Done", idle: "Idle" }[s] || s;
}

function relTime(ts) {
  const delta = (Date.now() - ts) / 1000;
  if (delta < 45) return "now";
  if (delta < 90) return "1m";
  if (delta < 3600) return `${Math.round(delta / 60)}m`;
  if (delta < 86400) return `${Math.round(delta / 3600)}h`;
  return `${Math.round(delta / 86400)}d`;
}

const UnreadBadge = ({ n, muted }) => {
  if (!n) return null;
  if (muted) {
    return <div style={{
      minWidth: 8, height: 8, borderRadius: "50%", background: "var(--text-mute)",
    }} />;
  }
  return (
    <div style={{
      minWidth: 18, height: 18, padding: "0 5px", borderRadius: 9,
      background: "var(--unread)", color: "white", fontSize: 11, fontWeight: 600,
      display: "flex", alignItems: "center", justifyContent: "center",
      fontVariantNumeric: "tabular-nums",
    }}>{n > 99 ? "99+" : n}</div>
  );
};

const SessionRow = ({ session, active, onClick, isNew }) => {
  const { avatar, name, short, status, unread, pinned, muted, lastPreview, lastSender, lastActive } = session;
  return (
    <div
      onClick={onClick}
      className={isNew ? "new-row" : ""}
      style={{
        display: "flex", gap: 10, alignItems: "flex-start",
        padding: `var(--density-row-y) 12px`,
        marginBottom: "var(--density-row-gap)",
        cursor: "pointer", position: "relative",
        background: active ? "var(--sidebar-active)" : "transparent",
        borderLeft: active ? "2px solid var(--accent)" : "2px solid transparent",
      }}
      onMouseEnter={(e) => { if (!active) e.currentTarget.style.background = "var(--sidebar-hover)"; }}
      onMouseLeave={(e) => { if (!active) e.currentTarget.style.background = "transparent"; }}
    >
      <Avatar av={avatar} status={status} kind={session.kind} group={avatar.group} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 2 }}>
          <div style={{
            fontSize: "var(--density-font)", fontWeight: unread ? 600 : 450,
            color: unread ? "var(--text-strong)" : "var(--text)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1, minWidth: 0,
          }}>{short}</div>
          {pinned && <Ic.pin style={{ color: "var(--text-mute)", flex: "0 0 auto" }} />}
          {muted && <Ic.mute style={{ color: "var(--text-mute)", flex: "0 0 auto" }} />}
          <div style={{ fontSize: 11, color: "var(--text-mute)", fontVariantNumeric: "tabular-nums", flex: "0 0 auto" }}>
            {relTime(lastActive)}
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <div style={{
            fontSize: 12, color: unread ? "var(--text)" : "var(--text-dim)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            fontFamily: "'JetBrains Mono', monospace", flex: 1, minWidth: 0,
            fontWeight: 400,
          }}>
            {status === "running" && !unread ? (
              <span style={{ color: "var(--status-running)" }}>
                <span className="typing-dot" /><span className="typing-dot" /><span className="typing-dot" />
                <span style={{ marginLeft: 6, color: "var(--text-dim)" }}>{truncate(lastPreview, 28)}</span>
              </span>
            ) : (
              <>
                {lastSender && lastSender !== "you" && (
                  <span style={{ color: "var(--text-mute)" }}>{lastSender}: </span>
                )}
                {truncate(lastPreview, 36)}
              </>
            )}
          </div>
          <UnreadBadge n={unread} muted={muted} />
        </div>
      </div>
    </div>
  );
};

function truncate(s, n) {
  if (!s) return "";
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

const Sidebar = ({ sessions, activeId, onSelect, onNew, onSearch, recentlyBumpedId, filter, setFilter }) => {
  const pinned = sessions.filter((s) => s.pinned);
  const unreadOrder = sessions.filter((s) => !s.pinned).sort((a, b) => b.lastActive - a.lastActive);

  const filtered = (arr) => {
    if (filter === "all") return arr;
    if (filter === "unread") return arr.filter((s) => s.unread > 0);
    if (filter === "agents") return arr.filter((s) => s.kind === "agent" || s.kind === "group");
    if (filter === "processes") return arr.filter((s) => s.kind === "process" || s.kind === "ci" || s.kind === "hook");
    return arr;
  };

  const totalUnread = sessions.reduce((a, s) => a + (s.muted ? 0 : s.unread), 0);

  return (
    <aside style={{
      width: 300, background: "var(--sidebar-bg)", borderRight: "1px solid var(--border)",
      display: "flex", flexDirection: "column", flex: "0 0 300px", minHeight: 0,
    }}>
      {/* Header */}
      <div className="no-select" style={{
        padding: "14px 14px 10px", borderBottom: "1px solid var(--border)",
      }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div style={{
              width: 22, height: 22, borderRadius: 5, background: "var(--accent)",
              display: "flex", alignItems: "center", justifyContent: "center", color: "white",
              fontFamily: "'JetBrains Mono', monospace", fontWeight: 700, fontSize: 11,
            }}>&gt;_</div>
            <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-strong)" }}>ChatTerm</div>
            {totalUnread > 0 && (
              <div style={{
                fontSize: 10, padding: "1px 6px", borderRadius: 8, background: "var(--unread)", color: "white",
                fontWeight: 600,
              }}>{totalUnread}</div>
            )}
          </div>
          <button onClick={onNew} title="New session (⌘N)" style={{
            background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer",
            padding: 4, borderRadius: 4, display: "flex",
          }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; }}
          ><Ic.plus /></button>
        </div>

        <button onClick={onSearch} style={{
          width: "100%", display: "flex", alignItems: "center", gap: 8,
          background: "#3c3c3c", border: "1px solid #3c3c3c",
          padding: "6px 10px", borderRadius: 4, cursor: "pointer", color: "var(--text-dim)",
          fontSize: 12, textAlign: "left",
        }}>
          <Ic.search />
          <span style={{ flex: 1 }}>Search sessions</span>
          <span className="mono" style={{ fontSize: 10, padding: "1px 5px", background: "#2d2d30", borderRadius: 3 }}>⌘K</span>
        </button>

        {/* Filter pills */}
        <div style={{ display: "flex", gap: 4, marginTop: 10, fontSize: 11 }}>
          {[
            { id: "all", label: "All" },
            { id: "unread", label: "Unread", n: totalUnread },
            { id: "agents", label: "Agents" },
            { id: "processes", label: "Processes" },
          ].map((f) => (
            <button key={f.id} onClick={() => setFilter(f.id)} style={{
              padding: "3px 8px", borderRadius: 10, border: "none", cursor: "pointer",
              background: filter === f.id ? "var(--accent)" : "transparent",
              color: filter === f.id ? "white" : "var(--text-dim)",
              fontSize: 11, fontWeight: 500,
              display: "flex", alignItems: "center", gap: 4,
            }}>
              {f.label}
              {f.n > 0 && <span style={{
                background: filter === f.id ? "rgba(255,255,255,0.25)" : "var(--unread)",
                color: "white", padding: "0 4px", borderRadius: 6, fontSize: 10,
              }}>{f.n}</span>}
            </button>
          ))}
        </div>
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", paddingTop: 6 }}>
        {filtered(pinned).length > 0 && (
          <>
            <SectionLabel>PINNED</SectionLabel>
            {filtered(pinned).map((s) => (
              <SessionRow key={s.id} session={s} active={s.id === activeId}
                onClick={() => onSelect(s.id)} isNew={s.id === recentlyBumpedId} />
            ))}
          </>
        )}
        <SectionLabel>RECENT</SectionLabel>
        {filtered(unreadOrder).map((s) => (
          <SessionRow key={s.id} session={s} active={s.id === activeId}
            onClick={() => onSelect(s.id)} isNew={s.id === recentlyBumpedId} />
        ))}
        {filtered(unreadOrder).length === 0 && filtered(pinned).length === 0 && (
          <div style={{ padding: "40px 20px", textAlign: "center", color: "var(--text-mute)", fontSize: 12 }}>
            No sessions match this filter.
          </div>
        )}
      </div>

      {/* Footer: self */}
      <div className="no-select" style={{
        borderTop: "1px solid var(--border)", padding: "10px 12px",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <div style={{
          width: 24, height: 24, borderRadius: 5, background: "var(--av-6)",
          color: "#1e1e1e", display: "flex", alignItems: "center", justifyContent: "center",
          fontFamily: "'JetBrains Mono',monospace", fontWeight: 700, fontSize: 11,
        }}>Y</div>
        <div style={{ flex: 1, fontSize: 12 }}>
          <div style={{ color: "var(--text-strong)", fontWeight: 500 }}>you</div>
          <div style={{ color: "var(--text-mute)", fontSize: 10 }}>~/work · feat/auth-refactor</div>
        </div>
        <Ic.settings style={{ color: "var(--text-dim)", cursor: "pointer" }} />
      </div>
    </aside>
  );
};

const SectionLabel = ({ children }) => (
  <div className="no-select" style={{
    padding: "10px 14px 6px", fontSize: 10, letterSpacing: 0.8, fontWeight: 600,
    color: "var(--text-mute)",
  }}>{children}</div>
);

Object.assign(window, { Sidebar, Avatar, statusColor, statusLabel, relTime, UnreadBadge });
