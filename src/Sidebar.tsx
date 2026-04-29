import React from "react";
import { Session, SessionAvatar, SessionKind, SessionStatus, statusColor, relTime, truncate, MOD_KEY } from "./types";
import { Ic } from "./Icons";
import { getCurrentTheme, subscribeTheme } from "./themes";
import { getPersona, subscribePersona } from "./persona";

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

interface AvatarProps {
  av: SessionAvatar;
  size?: number;
  status?: SessionStatus;
  asking?: boolean;
  group?: boolean;
  // Optional richer signals — only consulted in pet mode.
  kind?: SessionKind;
  thinking?: boolean;
  unread?: number;
  muted?: boolean;
}

export function Avatar(props: AvatarProps) {
  // Subscribe to persona at the leaf so callsites don't have to thread the
  // flag down. Re-renders only this Avatar instance when persona flips.
  const [persona, setPersona] = React.useState(getPersona);
  React.useEffect(() => subscribePersona(setPersona), []);
  if (persona === "pet") return <PetAvatar {...props} />;
  return <OperatorAvatar {...props} />;
}

function OperatorAvatar({ av, size = 36, status, asking, group }: AvatarProps) {
  const font = Math.round(size * 0.4);
  return (
    <div style={{ position: "relative", width: size, height: size, flex: `0 0 ${size}px` }}>
      <div style={{
        width: size, height: size, borderRadius: 6, background: av.color,
        color: "var(--avatar-text)", display: "flex", alignItems: "center", justifyContent: "center",
        fontFamily: "'JetBrains Mono', monospace", fontWeight: 700, fontSize: font,
        letterSpacing: -0.5, boxShadow: "inset 0 -1px 0 rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.12)",
      }}>{av.mono}</div>
      {group && (
        <div style={{
          position: "absolute", right: -3, bottom: -3, width: 14, height: 14, borderRadius: 4,
          background: "var(--av-4)", border: "2px solid var(--sidebar-bg)",
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 8, color: "var(--avatar-text)", fontWeight: 700, fontFamily: "'JetBrains Mono',monospace",
        }}>3</div>
      )}
      {status && !group && (
        <div title={asking ? "asking" : status} style={{
          position: "absolute", right: -2, bottom: -2, width: 10, height: 10, borderRadius: "50%",
          // Three-way priority: asking (user blocked) > running (output activity) > semantic status.
          // Running uses the avatar's own color so the pulse ring inherits it via `currentColor`;
          // asking overrides to red so a blocked agent can't be confused with an active one.
          // Running pulse is a fixed green (unified across sessions), not the
          // avatar's own color — the previous per-session hue made the signal
          // blend into the tile and get missed.
          background: asking
            ? "var(--status-asking)"
            : status === "running" ? "var(--status-running)" : statusColor(status),
          color: "var(--status-running)",
          border: "2px solid var(--sidebar-bg)",
        }} className={asking ? "pulse-asking" : status === "running" ? "pulse-running" : ""} />
      )}
    </div>
  );
}

// ─── Pet avatar ──────────────────────────────────────────────────────────
// Replaces the monogram tile with a kawaii creature whose species is keyed
// off `kind` (cat for agents, fox for ssh, …) and face is keyed off status.
// All runtime state still flows through the same Session shape — this is a
// pure visual swap, not a separate state model.

type PetSpeciesKey = "cat" | "fox" | "ham" | "pen" | "bun" | "owl";
type PetEyes = "focused" | "curious" | "x" | "happy" | "closed" | "dot";
type PetMouth = "smile" | "smirk" | "frown" | "o" | "neutral" | "gag";
type PetFaceState = SessionStatus | "asking";

// Pet mode treats species as decoration, not a kind indicator. Hashing the
// session's stable monogram into one of six species gives every session a
// pet but makes the bestiary feel varied — kinds are already conveyed by the
// KindIcon next to the title.
const PET_SPECIES: PetSpeciesKey[] = ["cat", "fox", "ham", "pen", "bun", "owl"];

function petSpeciesFor(seed: string): PetSpeciesKey {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
  return PET_SPECIES[Math.abs(h) % PET_SPECIES.length];
}

function petFaceFor(state: PetFaceState, muted: boolean): { eyes: PetEyes; mouth: PetMouth } {
  const mouth: PetMouth = muted ? "gag" : ({
    asking: "o", running: "smirk", error: "frown", done: "smile", idle: "neutral",
  } as Record<PetFaceState, PetMouth>)[state] || "neutral";
  const eyes: PetEyes = ({
    asking: "curious", running: "focused", error: "x", done: "happy", idle: "closed",
  } as Record<PetFaceState, PetEyes>)[state] || "dot";
  return { eyes, mouth };
}

function PetAvatar({ av, size = 36, status, asking, group, thinking, unread = 0, muted = false }: AvatarProps) {
  const species = petSpeciesFor(av.mono + av.color);
  const effective: PetFaceState = asking ? "asking" : (status || "idle");
  const face = petFaceFor(effective, muted);

  // Body wobble keyed off the dominant signal. Asking wins so a blocked
  // session always animates differently from a busy one.
  const wob =
    asking ? "pet-wave" :
    status === "running" ? "pet-wobble" :
    status === "error"   ? "pet-shake" :
    "";

  // Top-right emote bubble — only one shows; asking wins, then status, then
  // unread, then thinking-while-running.
  const emote = (() => {
    if (muted) return null;
    if (asking)             return { ch: "!", bg: "var(--status-asking)", color: "white", pulse: true };
    if (status === "error") return { ch: "!", bg: "var(--status-error)",  color: "white" };
    if (status === "done")  return { ch: "✓", bg: "var(--status-done)",   color: "var(--sidebar-bg)" };
    if (unread > 0)         return { ch: unread > 9 ? "9+" : String(unread), bg: "var(--unread)", color: "white" };
    if (status === "running" && thinking) return { ch: "…", bg: "var(--sidebar-bg)", color: "var(--text-strong)", border: true };
    if (status === "idle")  return { ch: "z", bg: "transparent", color: "var(--text-mute)", noborder: true, zzz: true };
    return null;
  })();

  return (
    <div style={{
      position: "relative", width: size, height: size, flex: `0 0 ${size}px`,
      filter: muted ? "saturate(0.25) brightness(0.85)" : "none",
    }}>
      <div className={wob} style={{
        width: size, height: size, borderRadius: "32%", background: av.color,
        boxShadow: "inset 0 -2px 0 rgba(0,0,0,0.18), inset 0 1px 0 rgba(255,255,255,0.18)",
        position: "relative", overflow: "visible",
      }}>
        <PetSpecies species={species} size={size} face={face} state={effective} />
      </div>

      {group && (
        <div style={{
          position: "absolute", right: -4, bottom: -4, width: 14, height: 14, borderRadius: "50%",
          background: "var(--av-4)", border: "2px solid var(--sidebar-bg)",
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 8, color: "var(--avatar-text)", fontWeight: 700, fontFamily: "'JetBrains Mono',monospace",
        }}>3</div>
      )}

      {emote && (
        <div className={`pet-bubble ${emote.pulse ? "pulse-asking" : ""} ${emote.zzz ? "pet-zzz" : ""}`}
          style={{
            position: "absolute", right: -6, top: -6,
            minWidth: 14, height: 14, padding: "0 3px", borderRadius: 7,
            background: emote.bg, color: emote.color,
            fontSize: 9, fontWeight: 800, fontFamily: "'JetBrains Mono', monospace",
            display: "flex", alignItems: "center", justifyContent: "center",
            border: emote.noborder ? "none" : "2px solid var(--sidebar-bg)",
            lineHeight: 1,
          }}>{emote.ch}</div>
      )}
    </div>
  );
}

function PetSpecies({ species, size, face, state }: { species: PetSpeciesKey; size: number; face: { eyes: PetEyes; mouth: PetMouth }; state: PetFaceState }) {
  // Species-specific ear/beak shapes layered on top of the round body.
  // Geometry is sized to a 36-unit canvas; SVG handles the scale.
  const ears = (() => {
    switch (species) {
      case "cat":
        return (<>
          <path d="M 6 8 L 9 2 L 13 7 Z" fill="rgba(0,0,0,0.25)"/>
          <path d="M 30 8 L 27 2 L 23 7 Z" fill="rgba(0,0,0,0.25)"/>
        </>);
      case "fox":
        return (<>
          <path d="M 5 9 L 7 1 L 13 8 Z" fill="rgba(255,255,255,0.7)" stroke="rgba(0,0,0,0.3)" strokeWidth="0.4"/>
          <path d="M 31 9 L 29 1 L 23 8 Z" fill="rgba(255,255,255,0.7)" stroke="rgba(0,0,0,0.3)" strokeWidth="0.4"/>
        </>);
      case "ham":
        return (<>
          <circle cx="8" cy="6" r="3" fill="rgba(0,0,0,0.18)"/>
          <circle cx="28" cy="6" r="3" fill="rgba(0,0,0,0.18)"/>
        </>);
      case "pen": return null;
      case "bun":
        return (<>
          <ellipse cx="12" cy="3" rx="2" ry="5" fill="rgba(255,255,255,0.55)" stroke="rgba(0,0,0,0.2)" strokeWidth="0.4"/>
          <ellipse cx="24" cy="3" rx="2" ry="5" fill="rgba(255,255,255,0.55)" stroke="rgba(0,0,0,0.2)" strokeWidth="0.4"/>
        </>);
      case "owl":
        return (<>
          <path d="M 5 7 L 9 2 L 13 7 Z" fill="rgba(0,0,0,0.22)"/>
          <path d="M 31 7 L 27 2 L 23 7 Z" fill="rgba(0,0,0,0.22)"/>
        </>);
    }
  })();
  const beak = species === "pen" && (<path d="M 17 22 L 19 22 L 18 24 Z" fill="#f2b33d"/>);
  return (
    <svg viewBox="0 0 36 36" width={size} height={size}
      style={{ position: "absolute", inset: 0, overflow: "visible" }}>
      {ears}
      <PetEyesEl face={face.eyes} state={state}/>
      <PetMouthEl face={face.mouth}/>
      {beak}
      {(face.mouth === "smile" || face.mouth === "smirk") && (
        <>
          <ellipse cx="11" cy="22" rx="1.6" ry="1" fill="rgba(255,120,120,0.35)"/>
          <ellipse cx="25" cy="22" rx="1.6" ry="1" fill="rgba(255,120,120,0.35)"/>
        </>
      )}
    </svg>
  );
}

function PetEyesEl({ face, state }: { face: PetEyes; state: PetFaceState }) {
  // Blink only when not actively running — staring eyes during a busy stream
  // read as "focused", blinking at rest reads as "alive".
  const blink = state === "running" ? "" : "pet-blink";
  switch (face) {
    case "focused":
      return (<g className={blink}>
        <circle cx="13" cy="18" r="1.8" fill="#1a1a1a"/>
        <circle cx="23" cy="18" r="1.8" fill="#1a1a1a"/>
        <circle cx="13.6" cy="17.4" r="0.5" fill="white"/>
        <circle cx="23.6" cy="17.4" r="0.5" fill="white"/>
      </g>);
    case "curious":
      return (<g>
        <circle cx="13" cy="18" r="2.6" fill="white" stroke="#1a1a1a" strokeWidth="0.6"/>
        <circle cx="23" cy="18" r="2.6" fill="white" stroke="#1a1a1a" strokeWidth="0.6"/>
        <circle cx="13" cy="18.5" r="1.2" fill="#1a1a1a"/>
        <circle cx="23" cy="18.5" r="1.2" fill="#1a1a1a"/>
      </g>);
    case "x":
      return (<g stroke="#1a1a1a" strokeWidth="1.4" strokeLinecap="round">
        <line x1="11" y1="16.5" x2="14.5" y2="20"/>
        <line x1="14.5" y1="16.5" x2="11" y2="20"/>
        <line x1="21.5" y1="16.5" x2="25" y2="20"/>
        <line x1="25" y1="16.5" x2="21.5" y2="20"/>
      </g>);
    case "happy":
      return (<g stroke="#1a1a1a" strokeWidth="1.4" strokeLinecap="round" fill="none">
        <path d="M 11 19 Q 13 16.5 15 19"/>
        <path d="M 21 19 Q 23 16.5 25 19"/>
      </g>);
    case "closed":
      return (<g className={blink} stroke="#1a1a1a" strokeWidth="1.4" strokeLinecap="round">
        <line x1="11" y1="18.5" x2="15" y2="18.5"/>
        <line x1="21" y1="18.5" x2="25" y2="18.5"/>
      </g>);
    default:
      return (<g className={blink}>
        <circle cx="13" cy="18" r="1.4" fill="#1a1a1a"/>
        <circle cx="23" cy="18" r="1.4" fill="#1a1a1a"/>
      </g>);
  }
}

function PetMouthEl({ face }: { face: PetMouth }) {
  switch (face) {
    case "smile":   return <path d="M 15 24 Q 18 27 21 24"     stroke="#1a1a1a" strokeWidth="1.2" fill="none" strokeLinecap="round"/>;
    case "smirk":   return <path d="M 16 24.5 Q 18 25.8 20 24.5" stroke="#1a1a1a" strokeWidth="1.1" fill="none" strokeLinecap="round"/>;
    case "frown":   return <path d="M 15 25.5 Q 18 23 21 25.5"  stroke="#1a1a1a" strokeWidth="1.2" fill="none" strokeLinecap="round"/>;
    case "o":       return <ellipse cx="18" cy="25" rx="1.2" ry="1.6" fill="#1a1a1a"/>;
    case "neutral": return <line x1="16.5" y1="25" x2="19.5" y2="25" stroke="#1a1a1a" strokeWidth="1.1" strokeLinecap="round"/>;
    case "gag":
      return (<g stroke="#1a1a1a" strokeWidth="1.1" strokeLinecap="round">
        <line x1="14.5" y1="25" x2="21.5" y2="25"/>
        <line x1="14.5" y1="25" x2="14.5" y2="23.6"/>
        <line x1="21.5" y1="25" x2="21.5" y2="23.6"/>
        <line x1="16.6" y1="24" x2="19.4" y2="26" stroke="#888" strokeWidth="0.7"/>
      </g>);
  }
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
      <Avatar av={s.avatar} status={s.status} asking={s.asking} group={s.avatar.group}
        kind={s.kind} thinking={s.thinking} unread={s.unread} muted={s.muted} />
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
            {s.thinking && !s.unread ? (
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
  // Track theme polarity so the theme-switch icon in the footer can render
  // the *opposite* of what's active — moon on light (nudge to dark), sun on
  // dark (nudge to light). Re-renders whenever a new theme is applied.
  const [isLight, setIsLight] = React.useState(() => !!getCurrentTheme().isLight);
  React.useEffect(() => subscribeTheme(t => setIsLight(!!t.isLight)), []);
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
                so the header logo reads as the same shape family. Bubble is
                wrapped in a scale transform so it doesn't fill the tile to
                the edges — breathing room around the chat glyph. The tile
                and the inner glyph share one `userSpaceOnUse` gradient so
                the diagonal blue→purple sweep reads as a single surface
                rather than three independent fills. */}
            <svg viewBox="0 0 1024 1024" width="26" height="26">
              <defs>
                <linearGradient id="ct-logo-grad" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="1024" y2="1024">
                  <stop offset="0%" stopColor="#5e8bff"/>
                  <stop offset="100%" stopColor="#a06cdd"/>
                </linearGradient>
              </defs>
              <rect width="1024" height="1024" rx="170" ry="170" fill="url(#ct-logo-grad)"/>
              <g transform="translate(512 512) scale(0.82) translate(-512 -512)">
                <path d="M 200 260 L 824 260 Q 864 260 864 300 L 864 680 Q 864 720 824 720 L 560 720 L 420 840 L 450 720 L 200 720 Q 160 720 160 680 L 160 300 Q 160 260 200 260 Z" fill="var(--logo-fg)"/>
                <path d="M 340 400 L 500 500 L 340 600" fill="none" stroke="url(#ct-logo-grad)" strokeWidth="56" strokeLinecap="round" strokeLinejoin="round"/>
                <rect x="540" y="570" width="180" height="40" rx="20" fill="url(#ct-logo-grad)"/>
              </g>
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
          background: "var(--sidebar-active)", border: "1px solid var(--border-strong)",
          padding: "6px 10px", borderRadius: 4, cursor: "pointer", color: "var(--text-dim)", fontSize: 12, textAlign: "left",
        }}>
          <Ic.search />
          <span style={{ flex: 1 }}>Search sessions</span>
          <span style={{ fontSize: 11, padding: "1px 5px", background: "var(--chip-bg)", borderRadius: 3, fontFamily: "-apple-system, sans-serif" }}>{MOD_KEY}K</span>
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
                background: "var(--unread)",
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
          width: 26, height: 26, borderRadius: 4, background: "var(--av-6)",
          color: "var(--avatar-text)", display: "flex", alignItems: "center", justifyContent: "center",
          fontFamily: "'JetBrains Mono',monospace", fontWeight: 700, fontSize: 11,
        }}>Y</div>
        <div style={{ flex: 1, minWidth: 0, display: "flex", alignItems: "baseline", gap: 6 }}>
          <div style={{ fontSize: 11, color: "var(--text-dim)" }}>Built for AI coding sessions.</div>
          <div className="mono" style={{ fontSize: 10, color: "var(--text-mute)" }}>v{__APP_VERSION__}</div>
        </div>
        {isLight
          ? <Ic.moon style={{ color: "var(--text-dim)", cursor: "pointer" }} onClick={onSettings} />
          : <Ic.sun  style={{ color: "var(--text-dim)", cursor: "pointer" }} onClick={onSettings} />}
      </div>
    </aside>
  );
}
