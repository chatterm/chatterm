import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";
import { PtyOutput } from "./types";
import { getCurrentTheme, toXtermTheme } from "./themes";

// Global cache: one Terminal instance per session, survives re-renders
const termCache = new Map<string, { term: Terminal; fit: FitAddon; unlisten: UnlistenFn | null }>();

function getOrCreate(sessionId: string): { term: Terminal; fit: FitAddon } {
  let entry = termCache.get(sessionId);
  if (entry) return entry;

  const term = new Terminal({
    fontSize: 14,
    fontFamily: "Menlo, 'SF Mono', Monaco, 'JetBrains Mono', ui-monospace, monospace",
    fontWeight: "normal",
    fontWeightBold: "bold",
    theme: toXtermTheme(getCurrentTheme()),
    cursorBlink: true,
    macOptionIsMeta: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);

  // Let Cmd+K/N pass through to app, block xterm input when overlays are open
  term.attachCustomKeyEventHandler((e) => {
    if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "n")) return false;
    if (document.querySelector(".cmdk-backdrop")) return false;
    return true;
  });

  // Keyboard → PTY
  term.onData((data) => {
    invoke("write_session", { id: sessionId, data });
  });

  // PTY → xterm (listen once, stays alive)
  let unlisten: UnlistenFn | null = null;
  listen<PtyOutput>("pty-output", (event) => {
    if (event.payload.session_id === sessionId) {
      term.write(event.payload.data);
    }
  }).then(fn => {
    unlisten = fn;
    const e = termCache.get(sessionId);
    if (e) e.unlisten = fn;
  });

  const newEntry = { term, fit, unlisten };
  termCache.set(sessionId, newEntry);
  return newEntry;
}

interface Props {
  sessionId: string;
}

export default function XtermPane({ sessionId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const { term, fit } = getOrCreate(sessionId);

    // Mount: attach to DOM
    if (!term.element) {
      term.open(container);
    } else {
      // Re-attach existing element
      while (container.firstChild) container.removeChild(container.firstChild);
      container.appendChild(term.element);
    }

    requestAnimationFrame(() => {
      fit.fit();
      term.focus();
      invoke("resize_session", { id: sessionId, cols: term.cols, rows: term.rows });
    });

    const ro = new ResizeObserver(() => {
      fit.fit();
      invoke("resize_session", { id: sessionId, cols: term.cols, rows: term.rows });
    });
    ro.observe(container);

    return () => {
      ro.disconnect();
      // Don't dispose — keep terminal alive for when user switches back
    };
  }, [sessionId]);

  return <div ref={containerRef} style={{ width: "100%", height: "100%", overflow: "hidden" }} />;
}

// Cleanup when session is killed
export function destroyTerminal(sessionId: string) {
  const entry = termCache.get(sessionId);
  if (entry) {
    entry.unlisten?.();
    entry.term.dispose();
    termCache.delete(sessionId);
  }
}
