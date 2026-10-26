import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";
import { PtyOutput, isMenuMod } from "./types";
import { getCurrentTheme, toXtermTheme, subscribeTheme } from "./themes";

// xterm paints its own selection overlay rather than a real DOM selection, so
// the webview's native Cmd+C and right-click "Copy" read an empty
// window.getSelection(). We copy term.getSelection() to the clipboard ourselves.
async function copyToClipboard(text: string) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Fallback for webviews that block the async clipboard API.
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand("copy"); } catch { /* nothing we can do */ }
    document.body.removeChild(ta);
  }
}

async function pasteFromClipboard(term: Terminal) {
  try {
    const text = await navigator.clipboard.readText();
    // term.paste respects bracketed-paste mode so a multi-line paste doesn't
    // run prematurely in the shell.
    if (text) term.paste(text);
  } catch { /* clipboard read blocked or empty */ }
}

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
      // Copy ourselves: xterm's selection is not a real DOM selection, so the
      // webview's native copy can't see it. Consume the copy gesture entirely
      // so a Cmd/Ctrl+Shift+C never leaks into the shell. Paste is left to
      // xterm's own textarea paste handler — intercepting it here would
      // double-paste, since returning false doesn't preventDefault.
      if (e.type === "keydown" && k === "c") {
        if (term.hasSelection()) copyToClipboard(term.getSelection());
        return false;
      }
    }
    if (document.querySelector(".cmdk-backdrop")) return false;
    // Windows IME fix: when an IME composition is active, block modifier
    // keys so the browser's default IME handling commits text correctly.
    // Note: Ctrl keydown sets e.ctrlKey=true in Chromium, so it's already
    // excluded by the outer guard — no need to check keyCode 17 here.
    if (e.isComposing && !e.ctrlKey && !e.metaKey) {
      const k = e.keyCode;
      // 16=Shift, 18=Alt, 20=CapsLock
      if (k === 16 || k === 18 || k === 20) return false;
    }
    return true;
  });

  // Keyboard → PTY
  term.onData((data) => {
    invoke("write_session", { id: sessionId, data });
  });

  // PTY → xterm (listen once, stays alive)
  // Coalesce writes per animation frame to avoid stutter in release builds
  // where the optimized PTY reader emits many small chunks.
  let unlisten: UnlistenFn | null = null;
  let pendingData = "";
  let rafId = 0;
  listen<PtyOutput>("pty-output", (event) => {
    if (event.payload.session_id === sessionId) {
      pendingData += event.payload.data;
      if (!rafId) {
        rafId = requestAnimationFrame(() => {
          term.write(pendingData);
          pendingData = "";
          rafId = 0;
        });
      }
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
  const [menu, setMenu] = useState<{ x: number; y: number; hasSel: boolean } | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const { term, fit } = getOrCreate(sessionId);

    // Mount: attach to DOM
    if (!term.element) {
      term.open(container);
      // Windows IME fix for Sogou and other third-party IMEs.
      //
      // Sogou has TWO different behaviors that both lose text:
      //
      // 1. Sogou Chinese mode → type English → Shift to commit:
      //    Sogou bypasses composition entirely. No compositionstart/end.
      //    It inserts text via a bare "input" event (inputType=insertText,
      //    isComposing=false). xterm.js may or may not handle this depending
      //    on internal state (_keyDownSeen). Sogou intercepts keydown events
      //    so xterm.js's _keyDownSeen is false → text lost.
      //
      // 2. Standard composition → compositionend with empty textarea:
      //    Some IMEs clear textarea.value before compositionend fires.
      //    xterm.js reads empty string via setTimeout(0) → text lost.
      //
      // Fix: listen for both patterns. Use defaultPrevented to detect if
      // xterm.js already handled the event (avoids double-send).
      const ta = term.textarea;
      if (ta && !(ta as any).__imePatched) {
        // Pattern 1: Sogou non-composition insertText
        ta.addEventListener("input", ((e: InputEvent) => {
          if (e.defaultPrevented) return;
          if (e.inputType !== "insertText" || e.isComposing || !e.data) return;
          // Sogou inserts multiple characters at once (e.g. "hello") via a
          // single input event. Normal keystrokes produce single-char input
          // events that xterm.js already handles via keydown. Only intercept
          // multi-char inserts to avoid double-sending normal typing.
          if (e.data.length <= 1) return;
          invoke("write_session", { id: sessionId, data: e.data });
          ta.value = "";
        }) as EventListener, true);

        // Pattern 2: compositionend with empty textarea
        ta.addEventListener("compositionend", (e: CompositionEvent) => {
          const text = e.data;
          if (!text) return;
          if (!ta.value || ta.value.trim() === "") {
            invoke("write_session", { id: sessionId, data: text });
          }
        }, true);

        (ta as any).__imePatched = true;
      }
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

    // Debounce fit/resize. A single layout change (split toggle, orientation
    // flip, window resize) fires ResizeObserver many times in one frame —
    // running `fit.fit()` and an IPC roundtrip on every callback noticeably
    // stutters when two xterms are mounted. Coalesce to one trailing call.
    let resizeTimer: number | undefined;
    const ro = new ResizeObserver(() => {
      if (resizeTimer !== undefined) clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => {
        fit.fit();
        invoke("resize_session", { id: sessionId, cols: term.cols, rows: term.rows });
      }, 60);
    });
    ro.observe(container);

    return () => {
      ro.disconnect();
      if (resizeTimer !== undefined) clearTimeout(resizeTimer);
      // Don't dispose — keep terminal alive for when user switches back
    };
  }, [sessionId]);

  return (
    <div
      ref={containerRef}
      onContextMenu={(e) => {
        // Native context menu can't read xterm's selection, so show our own.
        e.preventDefault();
        const term = termCache.get(sessionId)?.term;
        setMenu({ x: e.clientX, y: e.clientY, hasSel: !!term?.hasSelection() });
      }}
      style={{ width: "100%", height: "100%", overflow: "hidden" }}
    >
      {menu && (
        <TermContextMenu
          x={menu.x}
          y={menu.y}
          hasSel={menu.hasSel}
          onCopy={() => {
            const term = termCache.get(sessionId)?.term;
            if (term?.hasSelection()) copyToClipboard(term.getSelection());
            setMenu(null);
          }}
          onPaste={() => {
            const term = termCache.get(sessionId)?.term;
            if (term) pasteFromClipboard(term);
            setMenu(null);
          }}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

function TermContextMenu({ x, y, hasSel, onCopy, onPaste, onClose }: {
  x: number; y: number; hasSel: boolean;
  onCopy: () => void; onPaste: () => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Capture phase so a click anywhere (including inside xterm) is caught
    // before xterm can swallow it. Ignore clicks landing on the menu itself so
    // the item's onClick still runs.
    const close = (e: MouseEvent) => {
      if (ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    window.addEventListener("mousedown", close, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  const item: React.CSSProperties = {
    padding: "5px 14px", fontSize: 13, cursor: "pointer", whiteSpace: "nowrap",
    color: "var(--text)", fontFamily: "inherit",
  };
  return (
    <div
      ref={ref}
      style={{
        position: "fixed", left: x, top: y, zIndex: 1000, minWidth: 120,
        background: "var(--panel-bg)", border: "1px solid var(--border-strong)",
        borderRadius: 6, padding: "4px 0", boxShadow: "0 4px 16px rgba(0,0,0,0.35)",
      }}
    >
      <div
        style={{ ...item, opacity: hasSel ? 1 : 0.4, cursor: hasSel ? "pointer" : "default" }}
        onMouseEnter={(e) => { if (hasSel) e.currentTarget.style.background = "var(--sidebar-hover)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
        onClick={() => hasSel && onCopy()}
      >Copy</div>
      <div
        style={item}
        onMouseEnter={(e) => { e.currentTarget.style.background = "var(--sidebar-hover)"; }}
        onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
        onClick={onPaste}
      >Paste</div>
    </div>
  );
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
