// Right-side details panel. Shows session metadata, stats, quick actions, and members (for groups).

const DetailsPanel = ({ session, onClose, reactions }) => {
  const rxCount = Object.keys(reactions).filter(k => k.startsWith(session.id + ":")).length;

  return (
    <aside style={{
      width: 300, flex: "0 0 300px", background: "var(--panel-bg)",
      borderLeft: "1px solid var(--border)", display: "flex", flexDirection: "column",
      minHeight: 0,
    }}>
      <div className="no-select" style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "12px 16px", borderBottom: "1px solid var(--border)",
      }}>
        <div style={{ fontSize: 11, letterSpacing: 0.6, fontWeight: 600, color: "var(--text-mute)" }}>
          SESSION DETAILS
        </div>
        <button onClick={onClose} style={{
          background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer", padding: 4, display: "flex",
        }}><Ic.x /></button>
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "16px" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", paddingBottom: 14, borderBottom: "1px solid var(--border-strong)" }}>
          <Avatar av={session.avatar} size={56} status={session.status} group={session.avatar.group} />
          <div style={{ marginTop: 10, fontSize: 14, fontWeight: 600, color: "var(--text-strong)", textAlign: "center" }}>
            {session.name}
          </div>
          <div style={{ marginTop: 4 }}><StatusPill status={session.status} /></div>
        </div>

        {/* Members (group only) */}
        {session.members && (
          <Section title="MEMBERS">
            {session.members.map((m, i) => (
              <div key={i} style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 0" }}>
                <div style={{
                  width: 26, height: 26, borderRadius: 5, background: m.color,
                  color: "#1e1e1e", display: "flex", alignItems: "center", justifyContent: "center",
                  fontFamily: "'JetBrains Mono',monospace", fontWeight: 700, fontSize: 11,
                }}>{m.mono}</div>
                <div style={{ fontSize: 12, color: "var(--text)" }}>{m.role}</div>
              </div>
            ))}
          </Section>
        )}

        {/* Metadata */}
        <Section title="INFO">
          {session.cwd && <Row label="cwd" value={session.cwd} mono />}
          {session.branch && session.branch !== "—" && <Row label="branch" value={session.branch} mono color="var(--ansi-cyan)" />}
          {session.model && <Row label="model" value={session.model} mono color="var(--ansi-magenta)" />}
          {session.host && <Row label="host" value={`${session.user}@${session.host}`} mono />}
          {session.port && <Row label="port" value={session.port} mono />}
          {session.pid && <Row label="pid" value={session.pid} mono />}
          {session.uptime && <Row label="uptime" value={session.uptime} />}
          {session.duration && <Row label="duration" value={session.duration} />}
          {session.container && <Row label="container" value={session.container} mono />}
          {session.image && <Row label="image" value={session.image} mono />}
          {session.repo && <Row label="repo" value={session.repo} mono />}
          {session.commit && <Row label="commit" value={session.commit} mono />}
        </Section>

        {/* Tokens */}
        {session.tokens && (
          <Section title="TOKENS">
            <Row label="input" value={session.tokens.in.toLocaleString()} mono color="var(--ansi-cyan)" />
            <Row label="output" value={session.tokens.out.toLocaleString()} mono color="var(--ansi-magenta)" />
          </Section>
        )}

        {/* Tools */}
        {session.tools && (
          <Section title="TOOLS USED">
            {session.tools.map((t) => (
              <div key={t.name} style={{ display: "flex", alignItems: "center", gap: 8, padding: "3px 0" }}>
                <div style={{ flex: 1, fontFamily: "'JetBrains Mono',monospace", fontSize: 12, color: "var(--ansi-cyan)" }}>{t.name}</div>
                <div style={{
                  fontSize: 11, color: "var(--text-dim)", fontVariantNumeric: "tabular-nums",
                  padding: "1px 7px", background: "#2d2d30", borderRadius: 9,
                }}>×{t.count}</div>
              </div>
            ))}
          </Section>
        )}

        {/* Activity summary */}
        <Section title="ACTIVITY">
          <Row label="lines" value={session.lines.length} mono />
          <Row label="reactions" value={rxCount} mono />
          <Row label="last active" value={relTime(session.lastActive) + " ago"} />
        </Section>

        {/* Quick actions */}
        <Section title="ACTIONS">
          <ActionBtn>New input on this session</ActionBtn>
          <ActionBtn>Forward last output to another agent…</ActionBtn>
          <ActionBtn>Export transcript</ActionBtn>
          {session.kind === "agent" && <ActionBtn>Rerun last task with different model</ActionBtn>}
          {session.status === "running" && <ActionBtn danger>Kill process</ActionBtn>}
          {session.status === "idle" && <ActionBtn>Reconnect</ActionBtn>}
        </Section>
      </div>
    </aside>
  );
};

const Section = ({ title, children }) => (
  <div style={{ paddingTop: 16, paddingBottom: 4 }}>
    <div className="no-select" style={{
      fontSize: 10, letterSpacing: 0.7, fontWeight: 600, color: "var(--text-mute)", marginBottom: 6,
    }}>{title}</div>
    {children}
  </div>
);

const Row = ({ label, value, mono, color }) => (
  <div style={{ display: "flex", justifyContent: "space-between", gap: 8, padding: "3px 0", fontSize: 12 }}>
    <span style={{ color: "var(--text-mute)" }}>{label}</span>
    <span style={{
      color: color || "var(--text)",
      fontFamily: mono ? "'JetBrains Mono',monospace" : "inherit",
      overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: "60%",
      textAlign: "right",
    }}>{value}</span>
  </div>
);

const ActionBtn = ({ children, danger }) => (
  <button style={{
    width: "100%", textAlign: "left", padding: "6px 10px", marginBottom: 4,
    background: "transparent", border: "1px solid var(--border-strong)", borderRadius: 4,
    color: danger ? "var(--ansi-bright-red)" : "var(--text)", fontSize: 12, cursor: "pointer", fontFamily: "inherit",
  }}
    onMouseEnter={(e) => { e.currentTarget.style.background = "var(--sidebar-hover)"; }}
    onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
  >{children}</button>
);

Object.assign(window, { DetailsPanel });
