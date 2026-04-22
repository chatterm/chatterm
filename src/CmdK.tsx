import { useState, useEffect, useRef, useMemo } from "react";
import { Session } from "./types";
import { Ic } from "./Icons";
import { Avatar } from "./Sidebar";

function UnreadBadge({ n, muted }: { n: number; muted?: boolean }) {
  if (!n) return null;
  if (muted) return <div style={{ width: 8, height: 8, borderRadius: "50%", background: "var(--text-mute)" }} />;
  return <div style={{ minWidth: 18, height: 18, padding: "0 5px", borderRadius: 9, background: "var(--unread)", color: "white", fontSize: 11, fontWeight: 600, display: "flex", alignItems: "center", justifyContent: "center" }}>{n > 99 ? "99+" : n}</div>;
}

const Kbd = ({ children }: { children: string }) => (
  <span className="mono" style={{ padding: "1px 5px", background: "#2d2d30", borderRadius: 3, fontSize: 10, color: "var(--text-dim)" }}>{children}</span>
);

interface Props { sessions: Session[]; onClose: () => void; onSelect: (id: string) => void; }

export default function CmdK({ sessions, onClose, onSelect }: Props) {
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const ref = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const composing = useRef(false);
  useEffect(() => { ref.current?.focus(); }, []);

  // Keep the highlighted row visible when arrow-key navigation walks past the
  // scrollable container's edge. `block: "nearest"` scrolls the minimum amount
  // and is a no-op when the row is already in view.
  useEffect(() => {
    const row = listRef.current?.children[idx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [idx]);

  const results = useMemo(() => {
    if (!q.trim()) return sessions;
    const needle = q.toLowerCase();
    return sessions.filter(s =>
      s.name.toLowerCase().includes(needle) || s.short.toLowerCase().includes(needle) ||
      (s.cwd && s.cwd.toLowerCase().includes(needle)) || s.kind.includes(needle) ||
      s.lines.some(l => (l.text || "").toLowerCase().includes(needle))
    );
  }, [q, sessions]);

  const onKey = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
    if (e.key === "ArrowDown") { setIdx(i => Math.min(i + 1, results.length - 1)); e.preventDefault(); }
    if (e.key === "ArrowUp") { setIdx(i => Math.max(i - 1, 0)); e.preventDefault(); }
    // Skip Enter while IME is committing a candidate.
    if (composing.current || e.nativeEvent.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter") { if (results[idx]) { onSelect(results[idx].id); onClose(); } }
  };

  return (
    <div className="cmdk-backdrop" onClick={onClose}>
      <div className="cmdk-panel" onClick={e => e.stopPropagation()}>
        <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "12px 16px", borderBottom: "1px solid var(--border-strong)" }}>
          <Ic.search style={{ color: "var(--text-dim)" }} />
          <input ref={ref} value={q} onChange={e => { setQ(e.target.value); setIdx(0); }} onKeyDown={onKey}
            onCompositionStart={() => { composing.current = true; }}
            onCompositionEnd={() => { composing.current = false; }}
            placeholder="Search sessions, cwd, output…"
            style={{ flex: 1, background: "transparent", border: "none", outline: "none", color: "var(--text-strong)", fontSize: 15, fontFamily: "inherit" }}
          />
          <span className="mono" style={{ fontSize: 10, color: "var(--text-mute)", padding: "2px 6px", background: "#2d2d30", borderRadius: 3 }}>esc</span>
        </div>
        <div ref={listRef} style={{ maxHeight: 400, overflowY: "auto" }}>
          {results.length === 0 && <div style={{ padding: 30, textAlign: "center", color: "var(--text-mute)", fontSize: 13 }}>No matches.</div>}
          {results.map((s, i) => (
            <div key={s.id} onMouseEnter={() => setIdx(i)} onClick={() => { onSelect(s.id); onClose(); }}
              style={{
                display: "flex", gap: 10, alignItems: "center", padding: "8px 16px", cursor: "pointer",
                background: i === idx ? "var(--sidebar-active)" : "transparent",
                borderLeft: i === idx ? "2px solid var(--accent)" : "2px solid transparent",
              }}>
              <Avatar av={s.avatar} size={28} status={s.status} group={s.avatar.group} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, color: "var(--text-strong)", fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.name}</div>
                <div className="mono" style={{ fontSize: 11, color: "var(--text-dim)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.cwd || "—"}</div>
              </div>
              <div style={{ fontSize: 10, padding: "2px 6px", borderRadius: 3, background: "#2d2d30", color: "var(--text-dim)" }}>{s.kind}</div>
              <UnreadBadge n={s.unread} muted={s.muted} />
            </div>
          ))}
        </div>
        <div className="no-select" style={{ padding: "8px 16px", borderTop: "1px solid var(--border-strong)", fontSize: 11, color: "var(--text-mute)", display: "flex", gap: 16 }}>
          <span><Kbd>↑↓</Kbd> navigate</span>
          <span><Kbd>↵</Kbd> open</span>
          <span><Kbd>esc</Kbd> close</span>
        </div>
      </div>
    </div>
  );
}
