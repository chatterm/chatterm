// App — ties everything together. Handles state, live agent activity simulation,
// keyboard shortcuts, edit-mode protocol.

const App = () => {
  const [sessions, setSessions] = React.useState(() => window.SESSIONS.map(s => ({ ...s })));
  const [activeId, setActiveId] = React.useState("claude-refactor");
  const [reactions, setReactions] = React.useState(window.INITIAL_REACTIONS);
  const [cmdkOpen, setCmdkOpen] = React.useState(false);
  const [detailsOpen, setDetailsOpen] = React.useState(true);
  const [filter, setFilter] = React.useState("all");
  const [recentlyBumped, setRecentlyBumped] = React.useState(null);
  const [tweaksOpen, setTweaksOpen] = React.useState(false);
  const [tweaks, setTweaks] = React.useState(window.__TWEAKS__ || { tone: "neutral", density: "comfortable" });

  // Edit-mode protocol
  React.useEffect(() => {
    const handler = (e) => {
      const d = e.data;
      if (!d || !d.type) return;
      if (d.type === "__activate_edit_mode") setTweaksOpen(true);
      if (d.type === "__deactivate_edit_mode") setTweaksOpen(false);
    };
    window.addEventListener("message", handler);
    window.parent.postMessage({ type: "__edit_mode_available" }, "*");
    return () => window.removeEventListener("message", handler);
  }, []);

  // Apply tweaks to :root data attrs
  React.useEffect(() => {
    document.documentElement.setAttribute("data-tone", tweaks.tone);
    document.documentElement.setAttribute("data-density", tweaks.density);
  }, [tweaks]);

  // Keyboard shortcuts
  React.useEffect(() => {
    const handler = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") { e.preventDefault(); setCmdkOpen(true); }
      if (e.key === "Escape" && cmdkOpen) setCmdkOpen(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [cmdkOpen]);

  const activeSession = sessions.find((s) => s.id === activeId) || sessions[0];

  const onSelect = (id) => {
    setActiveId(id);
    // mark as read
    setSessions((cur) => cur.map((s) => s.id === id ? { ...s, unread: 0 } : s));
  };

  const onReact = (key, emoji) => {
    setReactions((cur) => {
      const row = { ...(cur[key] || {}) };
      row[emoji] = (row[emoji] || 0) + 1;
      return { ...cur, [key]: row };
    });
  };

  const togglePin = () => setSessions((cur) => cur.map((s) => s.id === activeId ? { ...s, pinned: !s.pinned } : s));
  const toggleMute = () => setSessions((cur) => cur.map((s) => s.id === activeId ? { ...s, muted: !s.muted } : s));

  const onSend = (text, quote) => {
    if (!text) return;
    setSessions((cur) => cur.map((s) => {
      if (s.id !== activeId) return s;
      const lines = [...s.lines];
      if (quote) {
        lines.push({ t: "sys", text: `quote: "${(quote.text || "").slice(0, 60)}"` });
      }
      lines.push({ t: "cmd", text });
      // simulate a response for agents
      if (s.kind === "agent") {
        lines.push({ t: "agent", text: "On it — let me take a look…" });
      }
      return { ...s, lines, lastActive: Date.now(), lastPreview: text, lastSender: "you" };
    }));
  };

  // Live simulation: periodically "bump" a running/agent session to the top
  // with a new line and unread count to show the IM-like behavior.
  React.useEffect(() => {
    const tick = () => {
      const roll = Math.random();
      setSessions((cur) => {
        const next = cur.map((s) => ({ ...s, lines: [...s.lines] }));

        // Dev-server new hmr update
        if (roll < 0.25) {
          const dv = next.find((s) => s.id === "dev-server");
          if (dv) {
            const time = new Date().toLocaleTimeString("en-GB", { hour12: false });
            dv.lines.push({ t: "out", text: `[${time}] hmr update /src/components/${pick(["Card","List","Row","Modal"])}.tsx`, color: "cyan" });
            dv.lastActive = Date.now();
            dv.lastPreview = "hmr update …";
          }
          return next;
        }
        // Postgres checkpoint
        if (roll < 0.4) {
          const pg = next.find((s) => s.id === "docker-logs");
          if (pg) {
            pg.lines.push({ t: "out", text: `2026-04-18 ${new Date().toLocaleTimeString("en-GB",{hour12:false})} UTC [82] LOG:  autovacuum: VACUUM public.sessions`, color: "gray" });
            pg.lastActive = Date.now();
            pg.lastPreview = "autovacuum: VACUUM …";
          }
          return next;
        }
        // Agent group progress
        if (roll < 0.55) {
          const gr = next.find((s) => s.id === "agent-group-migration");
          if (gr) {
            const lastProg = [...gr.lines].reverse().find((l) => l.t === "prog");
            const newPct = Math.min(100, (lastProg?.pct || 60) + Math.floor(Math.random() * 10 + 3));
            gr.lines.push({
              t: "prog", label: "Backfilling tokens", pct: newPct,
              detail: `${(newPct/100*2.3).toFixed(2)}M / 2.3M rows · eta ${Math.max(0, 40 - Math.floor((newPct-60)))}s`
            });
            gr.unread = (gr.unread || 0) + 1;
            gr.lastActive = Date.now();
            gr.lastPreview = `executor: ${newPct}% backfilled`;
            gr.lastSender = "executor";
            if (newPct === 100) {
              gr.lines.push({ t: "agent", who: "reviewer", color: "var(--av-7)", text: "All migrations applied cleanly on staging. Ready to promote to prod?" });
              gr.status = "waiting";
              gr.lastPreview = "reviewer: ready to promote to prod?";
              gr.lastSender = "reviewer";
            }
            if (gr.id !== activeId) {
              setRecentlyBumped("agent-group-migration");
              setTimeout(() => setRecentlyBumped(null), 500);
            }
          }
          return next;
        }
        // Occasional new agent finishes
        if (roll < 0.62) {
          const fi = next.find((s) => s.id === "codex-tests");
          if (fi && fi.status === "waiting") {
            // no-op; it's waiting on user
          }
        }
        return next;
      });
    };
    const h = setInterval(tick, 3200);
    return () => clearInterval(h);
  }, [activeId]);

  return (
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
      <Sidebar
        sessions={sessions}
        activeId={activeId}
        onSelect={onSelect}
        onNew={() => alert("New session (demo)")}
        onSearch={() => setCmdkOpen(true)}
        recentlyBumpedId={recentlyBumped}
        filter={filter}
        setFilter={setFilter}
      />
      <TerminalPane
        session={activeSession}
        reactions={reactions}
        onReact={onReact}
        onTogglePin={togglePin}
        onToggleMute={toggleMute}
        onOpenDetails={() => setDetailsOpen((v) => !v)}
        detailsOpen={detailsOpen}
        onSend={onSend}
      />
      {detailsOpen && (
        <DetailsPanel session={activeSession} onClose={() => setDetailsOpen(false)} reactions={reactions} />
      )}
      {cmdkOpen && (
        <CmdK sessions={sessions} onClose={() => setCmdkOpen(false)} onSelect={onSelect} />
      )}
      <TweaksPanel open={tweaksOpen} tweaks={tweaks} setTweaks={setTweaks} />
    </div>
  );
};

function pick(arr) { return arr[Math.floor(Math.random() * arr.length)]; }

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
