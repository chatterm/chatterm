// Cmd+K search overlay

const CmdK = ({ sessions, onClose, onSelect }) => {
  const [q, setQ] = React.useState("");
  const [idx, setIdx] = React.useState(0);
  const ref = React.useRef(null);

  React.useEffect(() => { ref.current?.focus(); }, []);

  const results = React.useMemo(() => {
    const base = sessions;
    if (!q.trim()) return base;
    const needle = q.toLowerCase();
    return base.filter((s) =>
      s.name.toLowerCase().includes(needle) ||
      s.short.toLowerCase().includes(needle) ||
      (s.cwd && s.cwd.toLowerCase().includes(needle)) ||
      (s.kind && s.kind.includes(needle)) ||
      s.lines.some((l) => (l.text || "").toLowerCase().includes(needle))
    );
  }, [q, sessions]);

  const onKey = (e) => {
    if (e.key === "Escape") onClose();
    if (e.key === "ArrowDown") { setIdx((i) => Math.min(i + 1, results.length - 1)); e.preventDefault(); }
    if (e.key === "ArrowUp") { setIdx((i) => Math.max(i - 1, 0)); e.preventDefault(); }
    if (e.key === "Enter") { if (results[idx]) { onSelect(results[idx].id); onClose(); } }
  };

  return (
    <div className="cmdk-backdrop" onClick={onClose}>
      <div className="cmdk-panel" onClick={(e) => e.stopPropagation()}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "12px 16px", borderBottom: "1px solid var(--border-strong)" }}>
          <Ic.search style={{ color: "var(--text-dim)" }} />
          <input ref={ref} value={q} onChange={(e) => { setQ(e.target.value); setIdx(0); }}
            onKeyDown={onKey}
            placeholder="Search sessions, cwd, output…"
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              color: "var(--text-strong)", fontSize: 15, fontFamily: "inherit",
            }}
          />
          <span className="mono" style={{ fontSize: 10, color: "var(--text-mute)", padding: "2px 6px", background: "#2d2d30", borderRadius: 3 }}>esc</span>
        </div>
        <div style={{ maxHeight: 400, overflowY: "auto" }}>
          {results.length === 0 && (
            <div style={{ padding: 30, textAlign: "center", color: "var(--text-mute)", fontSize: 13 }}>No matches.</div>
          )}
          {results.map((s, i) => (
            <div key={s.id}
              onMouseEnter={() => setIdx(i)}
              onClick={() => { onSelect(s.id); onClose(); }}
              style={{
                display: "flex", gap: 10, alignItems: "center",
                padding: "8px 16px", cursor: "pointer",
                background: i === idx ? "var(--sidebar-active)" : "transparent",
                borderLeft: i === idx ? "2px solid var(--accent)" : "2px solid transparent",
              }}
            >
              <Avatar av={s.avatar} size={28} status={s.status} group={s.avatar.group} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, color: "var(--text-strong)", fontWeight: 500,
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.name}</div>
                <div className="mono" style={{ fontSize: 11, color: "var(--text-dim)",
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.cwd || "—"}</div>
              </div>
              <div style={{ fontSize: 10, padding: "2px 6px", borderRadius: 3, background: "#2d2d30", color: "var(--text-dim)" }}>
                {s.kind}
              </div>
              {s.unread > 0 && <UnreadBadge n={s.unread} muted={s.muted} />}
            </div>
          ))}
        </div>
        <div className="no-select" style={{
          padding: "8px 16px", borderTop: "1px solid var(--border-strong)",
          fontSize: 11, color: "var(--text-mute)", display: "flex", gap: 16,
        }}>
          <span><Kbd>↑↓</Kbd> navigate</span>
          <span><Kbd>↵</Kbd> open</span>
          <span><Kbd>esc</Kbd> close</span>
        </div>
      </div>
    </div>
  );
};

const Kbd = ({ children }) => (
  <span className="mono" style={{ padding: "1px 5px", background: "#2d2d30", borderRadius: 3, fontSize: 10, color: "var(--text-dim)" }}>{children}</span>
);

Object.assign(window, { CmdK });
