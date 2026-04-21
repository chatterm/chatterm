import React from "react";
import { Session, SessionAvatar, SessionStatus, statusColor, relTime, truncate, MOD_KEY } from "./types";
import { Ic } from "./Icons";

// Full path + "$" when it fits the sidebar width; fall back to the last
// path segment + "$" when too long. Examples:
//   "/tmp"         → "/tmp$"
//   "/a/b/short"   → "/a/b/short$"
//   "/a/b/c/very-long-path" → "very-long-path$"
const SIDEBAR_CWD_MAX = 28;
function shortCwd(cwd: string): string {
  const full = `${cwd}$`;
  if (full.length <= SIDEBAR_CWD_MAX) return full;
  const seg = cwd.split("/").filter(Boolean).pop() || cwd;
  return `${seg}$`;
}

export function Avatar({ av, size = 36, status, group }: { av: SessionAvatar; size?: number; status?: SessionStatus; group?: boolean }) {
  const font = Math.round(size * 0.4);
  return (
    <div style={{ position: "relative", width: size, height: size, flex: `0 0 ${size}px` }}>
      <div style={{
        width: size, height: size, borderRadius: 6, background: av.color,
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
          background: statusColor(status), border: "2px solid var(--sidebar-bg)",
        }} className={status === "running" ? "pulse-running" : ""} />
      )}
    </div>
  );
}

function UnreadBadge({ n, muted }: { n: number; muted?: boolean }) {
  if (!n) return null;
  if (muted) return <div style={{ minWidth: 8, height: 8, borderRadius: "50%", background: "var(--text-mute)" }} />;
  return (
    <div style={{
      minWidth: 18, height: 18, padding: "0 5px", borderRadius: 9,
      background: "var(--unread)", color: "white", fontSize: 11, fontWeight: 600,
      display: "flex", alignItems: "center", justifyContent: "center", fontVariantNumeric: "tabular-nums",
    }}>{n > 99 ? "99+" : n}</div>
  );
}

function SessionRow({ session: s, active, onClick, onPin, onRename, onKill, onResume }: { session: Session; active: boolean; onClick: () => void; onPin: () => void; onRename: (name: string) => void; onKill?: () => void; onResume?: () => void }) {
  const [editing, setEditing] = React.useState(false);
  const [editName, setEditName] = React.useState(s.short);
  const [hover, setHover] = React.useState(false);
  const composing = React.useRef(false);
  return (
    <div onClick={onClick} onDoubleClick={(e) => { e.stopPropagation(); setEditing(true); setEditName(s.short); }}
      style={{
      display: "flex", gap: 10, alignItems: "flex-start",
      padding: "var(--density-row-y) 12px", marginBottom: "var(--density-row-gap)",
      cursor: "pointer", position: "relative",
      background: active ? "var(--sidebar-active)" : hover ? "var(--sidebar-hover)" : "transparent",
      borderLeft: active ? "2px solid var(--accent)" : "2px solid transparent",
    }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <Avatar av={s.avatar} status={s.status} group={s.avatar.group} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 2 }}>
          <div style={{
            fontSize: "var(--density-font)", fontWeight: 400,
            color: s.unread ? "var(--text-strong)" : "var(--text)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1, minWidth: 0,
          }}>{editing ? (
            <input value={editName} onChange={e => setEditName(e.target.value)}
              onBlur={() => { setEditing(false); if (editName.trim()) onRename(editName.trim()); }}
              onCompositionStart={() => { composing.current = true; }}
              onCompositionEnd={() => { composing.current = false; }}
              onKeyDown={e => {
                // Don't treat Enter as "submit rename" while an IME candidate
                // is being committed. Three signals, any one is enough:
                //   - composing ref (onCompositionStart/End)
                //   - e.nativeEvent.isComposing (Chromium standard)
                //   - keyCode 229 (legacy IME marker, still fires in WebKit)
                if (composing.current || e.nativeEvent.isComposing || e.keyCode === 229) return;
                if (e.key === "Enter") { setEditing(false); if (editName.trim()) onRename(editName.trim()); }
                if (e.key === "Escape") setEditing(false);
              }}
              onClick={e => e.stopPropagation()} autoFocus
              style={{ background: "var(--sidebar-hover)", border: "1px solid var(--accent)", borderRadius: 3, color: "var(--fg, var(--text))", fontSize: "var(--density-font)", fontWeight: 600, padding: "0 4px", width: "100%", outline: "none" }}
            />
          ) : s.short}</div>
          {s.pinned && !hover && <Ic.pin style={{ color: "var(--text-mute)", flex: "0 0 auto" }} />}
          {hover && (
            <div style={{ display: "flex", gap: 2, flex: "0 0 auto" }}>
              {onResume && <span onClick={(e) => { e.stopPropagation(); onResume(); }} title="Resume session"
                style={{ cursor: "pointer", padding: 2, borderRadius: 3, display: "flex" }}
                onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-active)"; }}
                onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
              ><Ic.resume style={{ color: "var(--text-mute)" }} /></span>}
              <span onClick={(e) => { e.stopPropagation(); onPin(); }} title={s.pinned ? "Unpin" : "Pin"}
                style={{ cursor: "pointer", padding: 2, borderRadius: 3, display: "flex" }}
                onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-active)"; }}
                onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
              ><Ic.pin style={{ color: s.pinned ? "var(--text)" : "var(--text-mute)" }} /></span>
              {onKill && <span onClick={(e) => { e.stopPropagation(); onKill(); }} title="Close session"
                style={{ cursor: "pointer", padding: 2, borderRadius: 3, display: "flex" }}
                onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-active)"; }}
                onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
              ><Ic.x style={{ color: "var(--text-mute)" }} /></span>}
            </div>
          )}
          <div style={{ fontSize: 11, color: "var(--text-mute)", fontVariantNumeric: "tabular-nums", flex: "0 0 auto" }}>
            {relTime(s.lastActive)}
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <div className="mono" style={{
            fontSize: 12, color: s.unread ? "var(--text)" : "var(--text-dim)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            flex: 1, minWidth: 0, fontWeight: 400,
          }}>
            {s.status === "running" && !s.unread ? (
              <span style={{ color: "var(--status-running)" }}>
                <span className="typing-dot" /><span className="typing-dot" /><span className="typing-dot" />
                <span style={{ marginLeft: 6, color: "var(--text-dim)" }}>{truncate(s.lastPreview || (s.cwd ? shortCwd(s.cwd) : ""), 28)}</span>
              </span>
            ) : s.lastPreview ? (
              <>
                {s.lastSender && s.lastSender !== "you" && (
                  <span style={{ color: "var(--text-mute)" }}>{s.lastSender}: </span>
                )}
                {truncate(s.lastPreview, 36)}
              </>
            ) : s.cwd ? (
              <span style={{ color: "var(--text-mute)" }}>{shortCwd(s.cwd)}</span>
            ) : null}
          </div>
          <UnreadBadge n={s.unread} muted={s.muted} />
        </div>
      </div>
    </div>
  );
}

const SectionLabel = ({ children }: { children: React.ReactNode }) => (
  <div className="no-select" style={{
    padding: "10px 14px 6px", fontSize: 10, letterSpacing: 0.8, fontWeight: 600, color: "var(--text-mute)",
  }}>{children}</div>
);

interface SidebarProps {
  sessions: Session[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onSearch: () => void;
  onPin: (id: string) => void;
  onRename: (id: string, name: string) => void;
  onKill: (id: string) => void;
  onResume: (id: string) => void;
  filter: string;
  setFilter: (f: string) => void;
  onSettings?: () => void;
}

export default function Sidebar({ sessions, activeId, onSelect, onNew, onSearch, onPin, onRename, onKill, onResume, filter, setFilter, onSettings }: SidebarProps) {
  const pinned = sessions.filter(s => s.pinned);
  const recent = sessions.filter(s => !s.pinned).sort((a, b) => b.lastActive - a.lastActive);
  const totalUnread = sessions.reduce((a, s) => a + (s.muted ? 0 : s.unread), 0);

  const fil = (arr: Session[]) => {
    if (filter === "all") return arr;
    if (filter === "unread") return arr.filter(s => s.unread > 0);
    if (filter === "agents") return arr.filter(s => s.kind === "agent" || s.kind === "group");
    if (filter === "processes") return arr.filter(s => s.kind === "process" || s.kind === "ci" || s.kind === "hook");
    return arr;
  };

  return (
    <aside style={{
      width: 300, background: "var(--sidebar-bg)", borderRight: "1px solid var(--border)",
      display: "flex", flexDirection: "column", flex: "0 0 300px", minHeight: 0,
    }}>
      {/* Header */}
      <div className="no-select" style={{ padding: "14px 14px 10px", borderBottom: "1px solid var(--border)" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {/* Corner radius matches the sidebar avatar tiles (~17% of side)
                so the header logo reads as the same shape family. */}
            <svg viewBox="0 0 1024 1024" width="22" height="22">
              <rect width="1024" height="1024" rx="170" ry="170" fill="#0e639c"/>
              <path d="M 200 260 L 824 260 Q 864 260 864 300 L 864 680 Q 864 720 824 720 L 560 720 L 420 840 L 450 720 L 200 720 Q 160 720 160 680 L 160 300 Q 160 260 200 260 Z" fill="#ffffff"/>
              <path d="M 340 400 L 500 500 L 340 600" fill="none" stroke="#0e639c" strokeWidth="56" strokeLinecap="round" strokeLinejoin="round"/>
              <rect x="540" y="570" width="180" height="40" rx="20" fill="#0e639c"/>
            </svg>
            <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-strong)" }}>ChatTerm</div>
            {totalUnread > 0 && (
              <div style={{
                fontSize: 10, padding: "1px 6px", borderRadius: 8, background: "var(--unread)", color: "white", fontWeight: 600,
              }}>{totalUnread}</div>
            )}
          </div>
          <button onClick={onNew} title={`New session (${MOD_KEY}N)`} style={{
            background: "transparent", border: "none", color: "var(--text-dim)", cursor: "pointer", padding: 4, borderRadius: 4, display: "flex",
          }}
            onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; }}
            onMouseLeave={e => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; }}
          ><Ic.plus /></button>
        </div>

        {/* Search button */}
        <button onClick={onSearch} style={{
          width: "100%", display: "flex", alignItems: "center", gap: 8,
          background: "#3c3c3c", border: "1px solid #3c3c3c",
          padding: "6px 10px", borderRadius: 4, cursor: "pointer", color: "var(--text-dim)", fontSize: 12, textAlign: "left",
        }}>
          <Ic.search />
          <span style={{ flex: 1 }}>Search sessions</span>
          <span style={{ fontSize: 11, padding: "1px 5px", background: "#2d2d30", borderRadius: 3, fontFamily: "-apple-system, sans-serif" }}>{MOD_KEY}K</span>
        </button>

        {/* Filter pills */}
        <div style={{ display: "flex", gap: 4, marginTop: 10, fontSize: 11 }}>
          {([
            { id: "all", label: "All" },
            { id: "unread", label: "Unread", n: totalUnread },
            { id: "agents", label: "Agents" },
            { id: "processes", label: "Processes" },
          ] as { id: string; label: string; n?: number }[]).map(f => (
            <button key={f.id} onClick={() => setFilter(f.id)} style={{
              padding: "3px 8px", borderRadius: 10, border: "none", cursor: "pointer",
              background: filter === f.id ? "var(--accent)" : "transparent",
              color: filter === f.id ? "white" : "var(--text-dim)",
              fontSize: 11, fontWeight: 500, display: "flex", alignItems: "center", gap: 4,
            }}>
              {f.label}
              {(f.n ?? 0) > 0 && <span style={{
                background: filter === f.id ? "rgba(255,255,255,0.25)" : "var(--unread)",
                color: "white", padding: "0 4px", borderRadius: 6, fontSize: 10,
              }}>{f.n}</span>}
            </button>
          ))}
        </div>
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", paddingTop: 6 }}>
        {fil(pinned).length > 0 && (
          <><SectionLabel>PINNED</SectionLabel>
          {fil(pinned).map(s => <SessionRow key={s.id} session={s} active={s.id === activeId} onClick={() => onSelect(s.id)} onPin={() => onPin(s.id)} onRename={(name) => onRename(s.id, name)} onKill={() => onKill(s.id)} onResume={s.kind === "agent" && s.status === "idle" ? () => onResume(s.id) : undefined} />)}</>
        )}
        <SectionLabel>RECENT</SectionLabel>
        {fil(recent).map(s => <SessionRow key={s.id} session={s} active={s.id === activeId} onClick={() => onSelect(s.id)} onPin={() => onPin(s.id)} onRename={(name) => onRename(s.id, name)} onKill={() => onKill(s.id)} onResume={s.kind === "agent" && s.status === "idle" ? () => onResume(s.id) : undefined} />)}
        {fil(recent).length === 0 && fil(pinned).length === 0 && (
          <div style={{ padding: "40px 20px", textAlign: "center", color: "var(--text-mute)", fontSize: 12 }}>
            {sessions.length === 0 ? "No sessions yet. Press + to create one." : "No sessions match this filter."}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="no-select" style={{
        borderTop: "1px solid var(--border)", padding: "10px 12px",
        display: "flex", alignItems: "center", gap: 8,
      }}>
        <div style={{
          width: 24, height: 24, borderRadius: 5, background: "var(--av-6)",
          color: "#1e1e1e", display: "flex", alignItems: "center", justifyContent: "center",
          fontFamily: "'JetBrains Mono',monospace", fontWeight: 700, fontSize: 11,
        }}>Y</div>
        <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "baseline", gap: 6 }}>
          <div style={{ fontSize: 11, color: "var(--text-dim)" }}>Built for AI coding sessions.</div>
          <div className="mono" style={{ fontSize: 10, color: "var(--text-mute)" }}>v0.1.0</div>
        </div>
        <Ic.settings style={{ color: "var(--text-dim)", cursor: "pointer" }} onClick={onSettings} />
      </div>
    </aside>
  );
}
