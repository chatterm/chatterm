export type SessionKind = "shell" | "agent" | "process" | "ssh" | "group" | "ci" | "hook";
export type SessionStatus = "running" | "done" | "error" | "idle";

export interface SessionAvatar { mono: string; color: string; group?: boolean; }

export interface OutputLine {
  t: "cmd" | "out" | "err" | "sys" | "agent" | "tool" | "diff" | "prog" | "done" | "ask";
  text: string;
  color?: string;
  inline?: boolean;
  who?: string;
  tool?: string;
  args?: string;
  diff?: { kind: string; text: string }[];
  pct?: number;
  detail?: string;
  label?: string;
  success?: boolean;
  summary?: { filesChanged: number; insertions: number; deletions: number; testsPassed: number };
  choices?: string[];
  raw?: boolean;
}

export interface Session {
  id: string;
  name: string;
  short: string;
  kind: SessionKind;
  avatar: SessionAvatar;
  status: SessionStatus;
  // Separate from `status` so activity (avatar pulse) and semantic thinking
  // state (sidebar typing-dot indicator) are independent signals. `status`
  // is driven by PTY output activity + 3s idle timer; `thinking` is driven
  // by backend vscreen regex matching / hook events. The thinking regex can
  // miss or mis-fire, but status stays reliable.
  thinking?: boolean;
  // Set when the agent is blocked on a confirmation dialog (permission
  // prompt, trust dialog, etc). Mutually exclusive with `thinking` — the
  // agent is NOT making progress, it's waiting for the user. Drives a
  // distinct (red) pulse separate from running's avatar-colour pulse.
  asking?: boolean;
  unread: number;
  pinned: boolean;
  muted: boolean;
  lastActive: number;
  lastPreview: string;
  lastSender?: string;
  lines: OutputLine[];
  cwd?: string;
  branch?: string;
  model?: string;
  host?: string;
  user?: string;
  port?: number;
  pid?: number;
}

export interface PtyOutput { session_id: string; data: string; }

// Platform-aware menu modifier. macOS uses Cmd; Linux uses Ctrl+Shift to
// avoid stealing bash readline (Ctrl+K/N) and vim (Ctrl+K digraph) shortcuts.
export const IS_MAC = typeof navigator !== "undefined"
  && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const MOD_KEY = IS_MAC ? "⌘" : "Ctrl+Shift+";
// True when the event carries the "menu" modifier (Cmd on macOS, Ctrl+Shift on Linux).
export const isMenuMod = (e: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) =>
  IS_MAC ? e.metaKey : (e.ctrlKey && e.shiftKey);

export const AVATAR_COLORS = [
  "var(--av-1)", "var(--av-2)", "var(--av-3)", "var(--av-4)",
  "var(--av-5)", "var(--av-6)", "var(--av-7)", "var(--av-8)",
];

export function statusColor(s: SessionStatus): string {
  return ({ running: "var(--status-running)", error: "var(--status-error)", done: "var(--status-done)", idle: "var(--status-idle)" } as Record<string, string>)[s] || "var(--status-idle)";
}

export function statusLabel(s: SessionStatus): string {
  return ({ running: "Running", error: "Error", done: "Done", idle: "Idle" } as Record<string, string>)[s] || s;
}

export function relTime(ts: number): string {
  const d = (Date.now() - ts) / 1000;
  if (d < 45) return "now";
  if (d < 90) return "1m";
  if (d < 3600) return `${Math.round(d / 60)}m`;
  if (d < 86400) return `${Math.round(d / 3600)}h`;
  return `${Math.round(d / 86400)}d`;
}

export function truncate(s: string, n: number): string {
  if (!s) return "";
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

// stripAnsi is in ansi.tsx
