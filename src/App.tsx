import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Sidebar from "./Sidebar";
import XtermPane from "./XtermPane";
import CmdK from "./CmdK";
import { Ic } from "./Icons";
import { Session, PtyOutput, AVATAR_COLORS, isMenuMod } from "./types";
import { loadSavedTheme, applyTheme, getAllThemes, getCurrentTheme, setCurrentTheme, addImportedTheme, TerminalTheme } from "./themes";
import { getPersona, setPersona, subscribePersona, Persona } from "./persona";
import "./App.css";

interface PtyMeta { session_id: string; title: string | null; agent: string | null; state: string | null; preview: string | null; command: string | null; cwd: string | null; }

const AGENT_INFO: Record<string, { mono: string; color: string; name: string }> = {
  claude: { mono: "CC", color: "var(--av-3)", name: "Claude Code" },
  codex:  { mono: "CX", color: "var(--av-4)", name: "Codex" },
  kiro:   { mono: "KR", color: "var(--av-2)", name: "Kiro CLI" },
};

let sessionCounter = 0;

interface SessionMeta { id: string; name: string; kind: string; agent: string | null; command: string | null; cwd: string | null; pinned: boolean; }

function makeSession(name: string): Session {
  const idx = sessionCounter++;
  return {
    id: `s${idx}`, name, short: name, kind: "shell",
    avatar: { mono: "SH", color: AVATAR_COLORS[idx % AVATAR_COLORS.length] },
    status: "idle", unread: 0, pinned: false, muted: false,
    lastActive: Date.now(), lastPreview: "", lines: [],
    cwd: "~",
  };
}

export default function App() {
  // Initialize theme on first render
  useState(() => applyTheme(loadSavedTheme()));

  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [filter, setFilter] = useState("all");
  const [cmdkOpen, setCmdkOpen] = useState(false);
  // const [recording, setRecording] = useState(false);
  const [themeOpen, setThemeOpen] = useState(false);
  const [systemThemes, setSystemThemes] = useState<string[]>([]);
  const sessionsRef = useRef<Session[]>([]);
  const activeIdRef = useRef<string | null>(null);
  const lastOutputRef = useRef<Record<string, number>>({});
  const initRef = useRef(false);

  sessionsRef.current = sessions;
  activeIdRef.current = activeId;

  // Persist session metadata whenever sessions change
  const persistSessions = useCallback((ss: Session[]) => {
    const metas: SessionMeta[] = ss.map(s => ({
      id: s.id, name: s.name, kind: s.kind, agent: (s as any)._agent || null,
      command: (s as any)._command || null, cwd: s.cwd || null, pinned: s.pinned,
    }));
    invoke("save_sessions", { sessions: metas }).catch(() => {});
  }, []);

  // Restore sessions on startup
  useEffect(() => {
    if (initRef.current) return;
    initRef.current = true;
    (async () => {
      try {
        const saved = await invoke<SessionMeta[]>("load_sessions");
        if (saved.length > 0) {
          let maxIdx = 0;
          const restored: Session[] = saved.map((m, i) => {
            const idx = parseInt(m.id.replace("s", "")) || i;
            if (idx >= maxIdx) maxIdx = idx + 1;
            const agentInfo = m.agent ? AGENT_INFO[m.agent] : null;
            return {
              id: m.id, name: m.name, short: m.name, kind: m.kind as any,
              avatar: agentInfo
                ? { mono: agentInfo.mono, color: agentInfo.color }
                : { mono: "SH", color: AVATAR_COLORS[i % AVATAR_COLORS.length] },
              status: "idle" as const, unread: 0, pinned: m.pinned, muted: false,
              lastActive: Date.now(), lastPreview: "", lines: [],
              cwd: m.cwd || "~",
              _agent: m.agent, _command: m.command,
            } as Session & { _agent?: string; _command?: string };
          });
          sessionCounter = maxIdx;
          setSessions(restored);
          setActiveId(restored[0]?.id || null);

          // Recreate PTY sessions — just open shell in cwd, don't auto-resume agents.
          // "~" is a shell alias, not a real path; pass null so the backend falls
          // back to $HOME. After create_session resolves we pull the live cwd via
          // `session_cwd` — the pty-meta listener may not be registered in time
          // for the backend's opportunistic initial emit.
          for (const m of saved) {
            const spawnCwd = m.cwd && m.cwd !== "~" ? m.cwd : null;
            invoke("create_session", { id: m.id, cols: 120, rows: 40, command: null, cwd: spawnCwd })
              .then(() => invoke<string | null>("session_cwd", { id: m.id }))
              .then(cwd => {
                if (cwd) setSessions(prev => prev.map(s => s.id === m.id ? { ...s, cwd } : s));
              })
              .catch(() => {});
          }
          return;
        }
      } catch {}
      // No saved sessions — create default shell
      createSession("Shell");
    })();
  }, []);

  sessionsRef.current = sessions;
  activeIdRef.current = activeId;

  // PTY output → treat any output as activity. `status === "running"` drives
  // the avatar pulse dot; the 3s idle timer below demotes back to idle. This
  // keeps the avatar signal rock-solid regardless of whether the vscreen
  // thinking regex matches.
  useEffect(() => {
    const unlisten = listen<PtyOutput>("pty-output", (event) => {
      const { session_id } = event.payload;
      lastOutputRef.current[session_id] = Date.now();
      // Only trigger a re-render when the session actually flips idle → running.
      setSessions(prev => {
        const target = prev.find(s => s.id === session_id);
        if (!target || target.status === "running") return prev;
        return prev.map(s => s.id === session_id ? { ...s, status: "running" } : s);
      });
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // PTY meta → detect agent, update avatar/name/state/preview
  useEffect(() => {
    const unlisten = listen<PtyMeta>("pty-meta", (event) => {
      const { session_id, agent, state, preview, command, cwd: metaCwd } = event.payload;

      setSessions(prev => prev.map(s => {
        if (s.id !== session_id) return s;
        const updates: Partial<Session> = {};

        // Agent detection → update avatar + name
        if (agent && s.kind !== "agent") {
          const info = AGENT_INFO[agent];
          if (info) {
            updates.kind = "agent";
            updates.name = info.name;
            updates.short = info.name;
            updates.avatar = { mono: info.mono, color: info.color };
            (updates as any)._agent = agent;
          }
        }

        // Capture agent command — but only when it belongs to the session's
        // current agent. Without this guard, a cross-agent match in the
        // backend (e.g. detecting codex while session is still labelled kiro)
        // could silently overwrite command with an unrelated process's args.
        if (command) {
          const currentAgent = (s as any)._agent;
          if (!currentAgent || !agent || agent === currentAgent) {
            (updates as any)._command = command;
          }
        }

        // Update cwd from hook
        if (metaCwd) {
          updates.cwd = metaCwd;
        }

        // Backend state (vscreen regex / hook) drives the semantic flags;
        // `status` stays owned by the output-activity path + idle timer so
        // flaky regex matches can't make the avatar pulse wrong. `asking`
        // (red pulse) and `thinking` (typing-dot) are mutually exclusive —
        // the agent is either making progress or waiting for the user.
        if (state === "asking") {
          updates.asking = true;
          updates.thinking = false;
        } else if (state === "thinking" || state === "working") {
          updates.thinking = true;
          updates.asking = false;
        } else if (state === "idle") {
          updates.thinking = false;
          updates.asking = false;
        }

        // Preview from Rust (already cleaned) — this means a real new message
        if (preview) {
          const isActive = session_id === activeIdRef.current;
          updates.lastPreview = preview;
          updates.lastActive = Date.now();
          if (!isActive && s.kind === "agent") {
            updates.unread = (s.unread || 0) + 1;
          }
        }

        return Object.keys(updates).length > 0 ? { ...s, ...updates } : s;
      }));
      // Persist when agent detected or cwd/command changed
      if (agent || metaCwd || command) { setTimeout(() => persistSessions(sessionsRef.current), 100); }
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  // Idle detection: if no output for 3s, mark as idle and clear thinking.
  // Thinking is also cleared here as a safety net — if the backend's "idle"
  // state regex doesn't match, we still stop showing the typing dots once
  // the PTY goes quiet.
  useEffect(() => {
    const timer = setInterval(() => {
      const now = Date.now();
      setSessions(prev => prev.map(s => {
        const lastOut = lastOutputRef.current[s.id] || 0;
        const stale = now - lastOut > 3000;
        if (!stale) return s;
        if (s.status !== "running" && !s.thinking) return s;
        return { ...s, status: "idle" as const, thinking: false };
      }));
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const createSession = useCallback(async (name: string, command?: string) => {
    const session = makeSession(name);
    (session as any)._command = command || null;
    setSessions(prev => { const next = [...prev, session]; persistSessions(next); return next; });
    setActiveId(session.id);
    try {
      await invoke("create_session", { id: session.id, cols: 120, rows: 40, command: command || null, cwd: null });
      // Pull initial cwd once the PTY is up (see restore branch for rationale).
      invoke<string | null>("session_cwd", { id: session.id })
        .then(cwd => {
          if (cwd) setSessions(prev => prev.map(s => s.id === session.id ? { ...s, cwd } : s));
        })
        .catch(() => {});
    } catch (e: any) {
      setSessions(prev => prev.map(s =>
        s.id === session.id ? { ...s, status: "error", lines: [...s.lines, { t: "err" as const, text: String(e) }] } : s
      ));
    }
  }, []);

  // Default shell creation is handled in the restore useEffect above

  const resumeSession = useCallback(async (id: string) => {
    const s = sessionsRef.current.find(x => x.id === id);
    if (!s) return;
    const agent = (s as any)._agent;
    const cmd = (s as any)._command;
    const AGENT_RESUME: Record<string, string> = {
      claude: "claude --resume", kiro: "kiro-cli chat --resume", codex: "codex resume --last",
    };
    let resumeCmd = "";
    if (agent && cmd) {
      if (cmd.includes("--resume") || cmd.includes("resume --last")) resumeCmd = cmd;
      else if (agent === "codex") resumeCmd = "codex resume --last";
      else if (agent === "kiro" && !cmd.includes(" chat")) resumeCmd = cmd + " chat --resume";
      else resumeCmd = cmd + " --resume";
    } else if (agent) {
      resumeCmd = AGENT_RESUME[agent] || "";
    }
    if (resumeCmd) {
      await invoke("write_session", { id, data: resumeCmd + "\n" });
    }
  }, []);

  const handleNew = () => createSession(`Shell ${sessions.length + 1}`);

  const handleSelect = (id: string) => {
    setActiveId(id);
    setSessions(prev => prev.map(s => s.id === id ? { ...s, unread: 0 } : s));
  };

  const toggleFullscreen = useCallback(async () => {
    try {
      const win = getCurrentWindow();
      await win.setFullscreen(!(await win.isFullscreen()));
    } catch (e) {
      console.error("Failed to toggle fullscreen", e);
    }
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "F11") { e.preventDefault(); toggleFullscreen(); return; }
      // Menu shortcuts: Cmd+X on macOS, Ctrl+Shift+X on Linux. Using plain Ctrl
      // on Linux would steal bash readline (Ctrl+K kill-line, Ctrl+N next-history)
      // and vim (Ctrl+K digraph, Ctrl+N autocomplete) inside shell sessions.
      if (isMenuMod(e)) {
        const k = e.key.toLowerCase();
        if (k === "k") { e.preventDefault(); setCmdkOpen(true); }
        if (k === "n") { e.preventDefault(); handleNew(); }
      }
      if (e.key === "Escape") {
        // Only swallow ESC when an overlay is actually open; otherwise let it
        // bubble to xterm so vim / Kiro CLI / anything else in the terminal
        // sees it.
        if (themeOpen) { e.preventDefault(); e.stopPropagation(); setThemeOpen(false); return; }
        if (cmdkOpen) { e.preventDefault(); e.stopPropagation(); setCmdkOpen(false); return; }
      }
    };
    window.addEventListener("keydown", handler, true); // capture phase
    return () => window.removeEventListener("keydown", handler, true);
  }, [cmdkOpen, themeOpen, sessions.length, toggleFullscreen]);

  const active = sessions.find(s => s.id === activeId);

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
      <Sidebar
        sessions={sessions} activeId={activeId} onSelect={handleSelect}
        onNew={handleNew} onSearch={() => setCmdkOpen(true)}
        onPin={(id) => setSessions(prev => { const next = prev.map(s => s.id === id ? { ...s, pinned: !s.pinned } : s); persistSessions(next); return next; })}
        onRename={(id, name) => setSessions(prev => prev.map(s => s.id === id ? { ...s, short: name, name } : s))}
        onKill={(id) => {
          invoke("kill_session", { id }).catch(() => {});
          setSessions(prev => {
            const next = prev.filter(s => s.id !== id);
            if (activeId === id) setActiveId(next[0]?.id || null);
            persistSessions(next);
            return next;
          });
        }}
        onResume={(id) => resumeSession(id)}
        filter={filter} setFilter={setFilter}
        onSettings={async () => {
          try { setSystemThemes(await invoke<string[]>("list_system_terminal_themes")); } catch {}
          setThemeOpen(true);
        }}
      />
      {active ? (
        <div style={{ flex: 1, display: "flex", flexDirection: "column", background: "var(--editor-bg)", minWidth: 0 }}>
          <div className="no-select" style={{
            display: "flex", alignItems: "center", gap: 10, padding: "10px 16px",
            borderBottom: "1px solid var(--border)", flex: "0 0 auto",
          }}>
            <div style={{
              width: 26, height: 26, borderRadius: 4, background: active.avatar.color,
              color: "var(--avatar-text)", display: "flex", alignItems: "center", justifyContent: "center",
              fontFamily: "'JetBrains Mono',monospace", fontWeight: 700, fontSize: 10,
            }}>{active.avatar.mono}</div>
            <div style={{ fontSize: 13, fontWeight: 400, color: "var(--text-strong)" }}>{active.name}</div>
            <span className="mono" style={{ fontSize: 11, color: "var(--text-dim)" }}>{active.kind}</span>
            {active.cwd && (
              <span className="mono" style={{
                fontSize: 11, color: "var(--text-dim)",
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                maxWidth: 400,
              }} title={active.cwd}>
                {active.cwd}
              </span>
            )}
            <div style={{ flex: 1 }} />
            <button
              onClick={toggleFullscreen}
              title="Toggle fullscreen (F11)"
              style={{
                width: 28, height: 28, flex: "0 0 28px",
                background: "transparent", border: "none", color: "var(--text-dim)",
                cursor: "pointer", padding: 0, borderRadius: 4,
                display: "flex", alignItems: "center", justifyContent: "center",
              }}
              onMouseEnter={e => { e.currentTarget.style.background = "var(--sidebar-hover)"; e.currentTarget.style.color = "var(--text)"; }}
              onMouseLeave={e => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-dim)"; }}
            >
              <Ic.fullscreen />
            </button>
            {/* Record button hidden
            <button
              onClick={async () => {
                const on = await invoke<boolean>("toggle_recording");
                setRecording(on);
              }}
              style={{
                background: recording ? "var(--ansi-bright-red)" : "transparent",
                border: recording ? "none" : "1px solid var(--border-strong)",
                color: recording ? "white" : "var(--text-dim)",
                borderRadius: 4, padding: "3px 10px", cursor: "pointer", fontSize: 11,
                fontFamily: "'JetBrains Mono',monospace", display: "flex", alignItems: "center", gap: 5,
              }}
            >
              <span style={{ width: 8, height: 8, borderRadius: "50%", background: recording ? "white" : "var(--ansi-bright-red)" }} />
              {recording ? "REC ●" : "Record"}
            </button>
            */}
          </div>
          <div style={{ flex: 1, overflow: "hidden" }}>
            <XtermPane key={active.id} sessionId={active.id} />
          </div>
        </div>
      ) : (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-mute)" }}>
          Press + to create a session
        </div>
      )}
      {cmdkOpen && <CmdK sessions={sessions} onClose={() => setCmdkOpen(false)} onSelect={handleSelect} />}
      {themeOpen && <ThemePanel
        systemThemes={systemThemes}
        onClose={() => setThemeOpen(false)}
      />}
    </div>
  );
}

function ThemePanel({ systemThemes, onClose }: { systemThemes: string[]; onClose: () => void }) {
  const [current, setCurrent] = useState(getCurrentTheme().name);
  const [persona, setPersonaState] = useState<Persona>(getPersona);
  useEffect(() => subscribePersona(setPersonaState), []);
  const themes = getAllThemes();

  const pick = (t: TerminalTheme) => {
    setCurrentTheme(t);
    setCurrent(t.name);
  };

  const importSystem = async (name: string) => {
    try {
      const t = await invoke<TerminalTheme>("export_system_terminal_theme", { name });
      addImportedTheme(t);
      pick(t);
    } catch (e) { console.error(e); }
  };

  return (
    <div className="cmdk-backdrop" onClick={onClose}>
      <div className="cmdk-panel" style={{ padding: 16, maxHeight: "70vh", overflow: "auto" }} onClick={e => e.stopPropagation()}>
        <div style={{ fontSize: 14, fontWeight: 600, color: "var(--text-strong)", marginBottom: 12 }}>Appearance</div>

        {/* Persona */}
        <div style={{ fontSize: 11, color: "var(--text-dim)", marginBottom: 6 }}>AVATAR STYLE</div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6, marginBottom: 14 }}>
          {([
            { v: "operator", label: "Operator", sub: "monogram tile" },
            { v: "pet",      label: "Pet 🐾",  sub: "kawaii creatures" },
          ] as { v: Persona; label: string; sub: string }[]).map(o => {
            const active = persona === o.v;
            return (
              <button key={o.v} onClick={() => setPersona(o.v)} style={{
                display: "flex", flexDirection: "column", alignItems: "stretch", gap: 4,
                padding: 8, borderRadius: 5, cursor: "pointer", textAlign: "left",
                background: active ? "var(--sidebar-active)" : "transparent",
                border: active ? "1px solid var(--accent)" : "1px solid var(--border-strong)",
                color: active ? "var(--text-strong)" : "var(--text-dim)",
              }}>
                <div style={{ fontSize: 12, fontWeight: 600 }}>{o.label}</div>
                <div style={{ fontSize: 10, color: "var(--text-mute)" }}>{o.sub}</div>
              </button>
            );
          })}
        </div>

        {/* Built-in + imported */}
        <div style={{ fontSize: 11, color: "var(--text-dim)", marginBottom: 6 }}>THEMES</div>
        {themes.map(t => (
          <button key={t.name} onClick={() => pick(t)} style={{
            width: "100%", display: "flex", alignItems: "center", gap: 10, padding: "8px 10px",
            background: t.name === current ? "var(--sidebar-active)" : "transparent",
            border: "none", borderRadius: 4, cursor: "pointer", color: "var(--text)", fontSize: 12, textAlign: "left",
          }}>
            <div style={{ display: "flex", gap: 2 }}>
              {[t.background, t.red, t.green, t.blue, t.yellow, t.cyan].map((c, i) => (
                <div key={i} style={{ width: 12, height: 12, borderRadius: 2, background: c }} />
              ))}
            </div>
            <span style={{ flex: 1 }}>{t.name}</span>
            {t.name === current && <span style={{ color: "var(--ansi-green)", fontSize: 11 }}>✓</span>}
          </button>
        ))}

        {/* System themes */}
        {systemThemes.length > 0 && <>
          <div style={{ fontSize: 11, color: "var(--text-dim)", marginTop: 14, marginBottom: 6 }}>IMPORT FROM macOS TERMINAL</div>
          <div style={{ maxHeight: 200, overflowY: "auto" }}>
            {systemThemes.filter(n => !themes.some(t => t.name === n)).map(name => (
              <button key={name} onClick={() => importSystem(name)} style={{
                width: "100%", display: "flex", alignItems: "center", gap: 10, padding: "6px 10px",
                background: "transparent", border: "none", borderRadius: 4, cursor: "pointer",
                color: "var(--text-dim)", fontSize: 12, textAlign: "left",
              }}>
                <span style={{ fontSize: 11 }}>↓</span>
                <span>{name}</span>
              </button>
            ))}
          </div>
        </>}
      </div>
    </div>
  );
}
