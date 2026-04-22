export interface TerminalTheme {
  name: string;
  // Terminal colors
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

// Derive UI colors from terminal theme
function lighten(hex: string, amt: number): string {
  const n = parseInt(hex.replace("#", ""), 16);
  const r = Math.min(255, ((n >> 16) & 0xff) + amt);
  const g = Math.min(255, ((n >> 8) & 0xff) + amt);
  const b = Math.min(255, (n & 0xff) + amt);
  return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, "0")}`;
}

export function deriveUI(t: TerminalTheme) {
  const bg = t.background;
  return {
    "--editor-bg": bg,
    "--sidebar-bg": lighten(bg, 7),
    "--sidebar-hover": lighten(bg, 12),
    "--sidebar-active": lighten(bg, 25),
    "--activity-bar": lighten(bg, 21),
    "--panel-bg": lighten(bg, 7),
    "--border": bg,
    "--border-strong": lighten(bg, 30),
    "--text": t.foreground,
    "--text-dim": t.brightBlack,
    "--text-mute": lighten(t.brightBlack, -20),
    "--text-strong": t.brightWhite,
    "--ansi-red": t.red, "--ansi-green": t.green, "--ansi-yellow": t.yellow,
    "--ansi-blue": t.blue, "--ansi-magenta": t.magenta, "--ansi-cyan": t.cyan,
    "--ansi-white": t.white, "--ansi-gray": t.brightBlack,
    "--ansi-bright-red": t.brightRed, "--ansi-bright-green": t.brightGreen,
    "--ansi-bright-yellow": t.brightYellow, "--ansi-bright-blue": t.brightBlue,
    "--ansi-bright-magenta": t.brightMagenta, "--ansi-bright-cyan": t.brightCyan,
    "--status-running": t.cyan,
    "--status-error": t.red, "--status-done": t.blue, "--status-idle": lighten(t.brightBlack, -20),
    "--status-asking": t.yellow,
    "--av-1": t.cyan, "--av-2": t.blue, "--av-3": t.magenta, "--av-4": t.yellow,
    "--av-5": lighten(t.red, 30), "--av-6": lighten(t.cyan, 40),
    "--av-7": t.green, "--av-8": t.red,
  };
}

// xterm.js ITheme object from our theme
export function toXtermTheme(t: TerminalTheme) {
  const { name, ...colors } = t;
  return colors;
}

// --- Built-in themes ---

export const chatterm: TerminalTheme = {
  name: "ChatTerm",
  background: "#1a1d22", foreground: "#c8ccd4", cursor: "#ffffff",
  selectionBackground: "#2e4563",
  black: "#1a1d22", red: "#e06c75", green: "#98c379", yellow: "#e5c07b",
  blue: "#61afef", magenta: "#c678dd", cyan: "#56b6c2", white: "#abb2bf",
  brightBlack: "#5c6370", brightRed: "#f44747", brightGreen: "#89d185",
  brightYellow: "#f9f1a5", brightBlue: "#3794ff", brightMagenta: "#d670d6",
  brightCyan: "#29b8db", brightWhite: "#ffffff",
};

export const vscodeDark: TerminalTheme = {
  name: "VS Code Dark",
  background: "#1e1e1e", foreground: "#cccccc", cursor: "#ffffff",
  selectionBackground: "#264f78",
  black: "#000000", red: "#f48771", green: "#b5cea8", yellow: "#dcdcaa",
  blue: "#569cd6", magenta: "#c586c0", cyan: "#4ec9b0", white: "#d4d4d4",
  brightBlack: "#858585", brightRed: "#f14c4c", brightGreen: "#89d185",
  brightYellow: "#f9f1a5", brightBlue: "#3794ff", brightMagenta: "#d670d6",
  brightCyan: "#29b8db", brightWhite: "#ffffff",
};

export const vscodeDarkPlus: TerminalTheme = {
  name: "VS Code Dark+ (macOS Terminal)",
  background: "#141414", foreground: "#c0c0c0", cursor: "#ffffff",
  selectionBackground: "#264f78",
  black: "#000000", red: "#d3001d", green: "#00b75f", yellow: "#dde600",
  blue: "#0059c3", magenta: "#be00b4", cyan: "#009ac6", white: "#dedede",
  brightBlack: "#535353", brightRed: "#ff1b34", brightGreen: "#00d171",
  brightYellow: "#f1fa00", brightBlue: "#1176ed", brightMagenta: "#dc45d3",
  brightCyan: "#00acd7", brightWhite: "#dedede",
};

export const builtinThemes: TerminalTheme[] = [chatterm, vscodeDark, vscodeDarkPlus];

// --- Imported themes (persisted in localStorage) ---

const IMPORTED_KEY = "chatterm-imported-themes";

function loadImportedThemes(): TerminalTheme[] {
  try {
    const raw = localStorage.getItem(IMPORTED_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveImportedThemes(themes: TerminalTheme[]) {
  try { localStorage.setItem(IMPORTED_KEY, JSON.stringify(themes)); } catch {}
}

export function getAllThemes(): TerminalTheme[] {
  return [...builtinThemes, ...loadImportedThemes()];
}

export function addImportedTheme(t: TerminalTheme): TerminalTheme {
  const imported = loadImportedThemes();
  // Replace if same name exists
  const idx = imported.findIndex(x => x.name === t.name);
  if (idx >= 0) imported[idx] = t; else imported.push(t);
  saveImportedThemes(imported);
  return t;
}

export function removeImportedTheme(name: string) {
  const imported = loadImportedThemes().filter(t => t.name !== name);
  saveImportedThemes(imported);
}

// Apply theme to document CSS variables
export function applyTheme(t: TerminalTheme) {
  const vars = deriveUI(t);
  const root = document.documentElement;
  for (const [k, v] of Object.entries(vars)) {
    root.style.setProperty(k, v);
  }
}

// Current theme state
let currentTheme: TerminalTheme = chatterm;

export function getCurrentTheme(): TerminalTheme { return currentTheme; }

export function setCurrentTheme(t: TerminalTheme) {
  currentTheme = t;
  applyTheme(t);
  try { localStorage.setItem("chatterm-theme", t.name); } catch {}
}

export function loadSavedTheme(): TerminalTheme {
  try {
    const name = localStorage.getItem("chatterm-theme");
    if (name) {
      const found = getAllThemes().find(t => t.name === name);
      if (found) { currentTheme = found; return found; }
    }
  } catch {}
  return currentTheme;
}
