import React from "react";

// ANSI 16-color map → CSS variables from App.css
const COLORS_16: Record<number, string> = {
  30: "var(--ansi-black,#000)", 31: "var(--ansi-red)", 32: "var(--ansi-green)",
  33: "var(--ansi-yellow)", 34: "var(--ansi-blue)", 35: "var(--ansi-magenta)",
  36: "var(--ansi-cyan)", 37: "var(--ansi-white)",
  90: "var(--ansi-gray)", 91: "var(--ansi-bright-red)", 92: "var(--ansi-bright-green)",
  93: "var(--ansi-bright-yellow)", 94: "var(--ansi-bright-blue)", 95: "var(--ansi-bright-magenta)",
  96: "var(--ansi-bright-cyan)", 97: "var(--text-strong,#fff)",
};

const BG_16: Record<number, string> = {
  40: "var(--ansi-black,#000)", 41: "var(--ansi-red)", 42: "var(--ansi-green)",
  43: "var(--ansi-yellow)", 44: "var(--ansi-blue)", 45: "var(--ansi-magenta)",
  46: "var(--ansi-cyan)", 47: "var(--ansi-white)",
  100: "var(--ansi-gray)", 101: "var(--ansi-bright-red)", 102: "var(--ansi-bright-green)",
  103: "var(--ansi-bright-yellow)", 104: "var(--ansi-bright-blue)", 105: "var(--ansi-bright-magenta)",
  106: "var(--ansi-bright-cyan)", 107: "var(--text-strong,#fff)",
};

// 256-color palette (standard 6x6x6 cube + grayscale)
function color256(n: number): string {
  if (n < 16) {
    const map: Record<number, string> = {
      0: "#000", 1: "#aa0000", 2: "#00aa00", 3: "#aa5500", 4: "#0000aa",
      5: "#aa00aa", 6: "#00aaaa", 7: "#aaaaaa", 8: "#555555", 9: "#ff5555",
      10: "#55ff55", 11: "#ffff55", 12: "#5555ff", 13: "#ff55ff", 14: "#55ffff", 15: "#ffffff",
    };
    return map[n] || "#fff";
  }
  if (n < 232) {
    const i = n - 16;
    const r = Math.floor(i / 36) * 51;
    const g = Math.floor((i % 36) / 6) * 51;
    const b = (i % 6) * 51;
    return `rgb(${r},${g},${b})`;
  }
  const v = (n - 232) * 10 + 8;
  return `rgb(${v},${v},${v})`;
}

interface Style {
  color?: string;
  bg?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikethrough?: boolean;
}

function applyParams(style: Style, params: number[]): Style {
  const s = { ...style };
  let i = 0;
  while (i < params.length) {
    const p = params[i];
    if (p === 0) { return {}; } // reset
    if (p === 1) s.bold = true;
    else if (p === 2) s.dim = true;
    else if (p === 3) s.italic = true;
    else if (p === 4) s.underline = true;
    else if (p === 9) s.strikethrough = true;
    else if (p === 22) { s.bold = false; s.dim = false; }
    else if (p === 23) s.italic = false;
    else if (p === 24) s.underline = false;
    else if (p === 29) s.strikethrough = false;
    else if (p === 39) s.color = undefined;
    else if (p === 49) s.bg = undefined;
    else if (COLORS_16[p]) s.color = COLORS_16[p];
    else if (BG_16[p]) s.bg = BG_16[p];
    else if (p === 38 && params[i + 1] === 5) { s.color = color256(params[i + 2] ?? 0); i += 2; }
    else if (p === 48 && params[i + 1] === 5) { s.bg = color256(params[i + 2] ?? 0); i += 2; }
    else if (p === 38 && params[i + 1] === 2) { s.color = `rgb(${params[i+2]??0},${params[i+3]??0},${params[i+4]??0})`; i += 4; }
    else if (p === 48 && params[i + 1] === 2) { s.bg = `rgb(${params[i+2]??0},${params[i+3]??0},${params[i+4]??0})`; i += 4; }
    i++;
  }
  return s;
}

function styleToCSS(s: Style): React.CSSProperties | undefined {
  if (!s.color && !s.bg && !s.bold && !s.dim && !s.italic && !s.underline && !s.strikethrough) return undefined;
  const css: React.CSSProperties = {};
  if (s.color) css.color = s.color;
  if (s.bg) css.backgroundColor = s.bg;
  if (s.bold) css.fontWeight = 700;
  if (s.dim) css.opacity = 0.6;
  if (s.italic) css.fontStyle = "italic";
  const deco: string[] = [];
  if (s.underline) deco.push("underline");
  if (s.strikethrough) deco.push("line-through");
  if (deco.length) css.textDecoration = deco.join(" ");
  return css;
}

// Regex: match CSI sequences \033[ ... m  and OSC sequences \033] ... \007
const ANSI_RE = /\x1b\[([0-9;]*)([a-zA-Z])|\x1b\][^\x07]*\x07|\x1b[()][AB012]/g;

/**
 * Parse a string with ANSI escape sequences into React elements.
 * Only handles SGR (color/style) sequences; strips cursor movement, OSC, etc.
 */
export function parseAnsi(raw: string): React.ReactNode {
  if (!raw || !raw.includes("\x1b")) {
    return raw || "\u00a0";
  }

  const parts: React.ReactNode[] = [];
  let style: Style = {};
  let lastIndex = 0;
  let key = 0;

  ANSI_RE.lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = ANSI_RE.exec(raw)) !== null) {
    // Text before this escape
    if (match.index > lastIndex) {
      const text = raw.slice(lastIndex, match.index);
      if (text) {
        const css = styleToCSS(style);
        parts.push(css ? <span key={key++} style={css}>{text}</span> : text);
      }
    }
    lastIndex = match.index + match[0].length;

    // Only process SGR sequences (ending with 'm')
    if (match[2] === "m" && match[1] !== undefined) {
      const params = match[1] ? match[1].split(";").map(Number) : [0];
      style = applyParams(style, params);
    }
    // All other sequences (cursor movement, OSC, etc.) are silently stripped
  }

  // Remaining text
  if (lastIndex < raw.length) {
    const text = raw.slice(lastIndex);
    if (text) {
      const css = styleToCSS(style);
      parts.push(css ? <span key={key++} style={css}>{text}</span> : text);
    }
  }

  return parts.length === 0 ? "\u00a0" : parts.length === 1 ? parts[0] : <>{parts}</>;
}

/**
 * Strip ANSI but keep the text content (for previews, search, etc.)
 */
export function stripAnsi(s: string): string {
  return s.replace(ANSI_RE, "").replace(/[\r]/g, "").trim();
}
