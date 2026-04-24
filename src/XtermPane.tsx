import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";
import { PtyOutput, isMenuMod } from "./types";
import { getCurrentTheme, toXtermTheme, subscribeTheme } from "./themes";

// Global cache: one Terminal instance per session, survives re-renders
const termCache = new Map<string, { term: Terminal; fit: FitAddon; unlisten: UnlistenFn | null; unsubTheme: () => void }>();

function getOrCreate(sessionId: string): { term: Terminal; fit: FitAddon } {
  let entry = termCache.get(sessionId);
  if (entry) return entry;

  const term = new Terminal({
    fontSize: 14,
    // Nerd Font variants first so prompts with Powerline / devicon glyphs
    // render correctly when the user has a Nerd Font installed; plain mono
    // families after as fallback.
    fontFamily:
      "'JetBrainsMono Nerd Font Mono', 'JetBrainsMono Nerd Font'," +
      "'FiraCode Nerd Font Mono', 'Hack Nerd Font Mono'," +
      "Menlo, 'SF Mono', Monaco, 'JetBrains Mono', ui-monospace, monospace",
    fontWeight: "normal",
    fontWeightBold: "bold",
    theme: toXtermTheme(getCurrentTheme()),
    cursorBlink: true,
    macOptionIsMeta: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);

  // Let the app's menu shortcuts (Cmd+X on macOS, Ctrl+Shift+X on Linux) pass
  // through to the global keydown handler. Bare Ctrl on Linux stays in xterm so
  // bash readline and vim keep working; also block xterm input while overlays are open.
  term.attachCustomKeyEventHandler((e) => {
    if (isMenuMod(e)) {
      const k = e.key.toLowerCase();
      if (k === "k" || k === "n") return false;
    }
    if (document.querySelector(".cmdk-backdrop")) return false;
    // Windows IME fix: when an IME composition is active (e.g. typing
    // pinyin with Sogou/MS input method), pressing Shift to switch to
    // English should commit the composition buffer. But xterm.js
    // intercepts the bare Shift keydown and cancels the composition,
    // losing the typed text. Block Shift/Control/Alt during composition
    // so the browser's default IME handling commits the text correctly.
    // Non-modifier keys (Enter, digits, Esc) must pass through so
    // xterm.js CompositionHelper can finalize the composition normally.
    if (e.isComposing && !e.ctrlKey && !e.metaKey) {
      const k = e.keyCode;
      // 16=Shift, 17=Ctrl, 18=Alt, 20=CapsLock
      if (k === 16 || k === 17 || k === 18 || k === 20) return false;
    }
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

  // Live theme switches: tied to the terminal's cache lifetime, not the
  // component mount. Otherwise the inactive session's term keeps its old
  // palette when the user toggles theme from a different tab.
  const unsubTheme = subscribeTheme((t) => {
    term.options.theme = toXtermTheme(t);
  });

  const newEntry = { term, fit, unlisten, unsubTheme };
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
    entry.unsubTheme();
    entry.term.dispose();
    termCache.delete(sessionId);
  }
}
