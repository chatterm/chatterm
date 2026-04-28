//! Virtual screen buffer: feeds PTY output through a VTE parser,
//! maintains a grid of characters, and allows reading screen content
//! for extracting meaningful text from TUI applications.

use vte::{Params, Parser, Perform};

const COLS: usize = 200;
const ROWS: usize = 60;

pub struct VScreen {
    parser: Parser,
    inner: ScreenInner,
}

struct ScreenInner {
    grid: Vec<Vec<char>>,
    cursor_row: usize,
    cursor_col: usize,
    rows: usize,
    cols: usize,
    /// True when the terminal has switched to the alternate screen buffer
    /// (CSI ?1049h or CSI ?47h). TUI apps like nvim, vim, htop use this.
    alt_screen: bool,
}

impl ScreenInner {
    fn new() -> Self {
        Self {
            grid: vec![vec![' '; COLS]; ROWS],
            cursor_row: 0,
            cursor_col: 0,
            rows: ROWS,
            cols: COLS,
            alt_screen: false,
        }
    }

    fn clear_line(&mut self, row: usize) {
        if row < self.rows {
            self.grid[row] = vec![' '; self.cols];
        }
    }

    /// Get a specific row as trimmed string
    fn row_text(&self, row: usize) -> String {
        if row >= self.rows {
            return String::new();
        }
        self.grid[row]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Get all non-empty rows as strings
    fn all_rows(&self) -> Vec<String> {
        (0..self.rows)
            .map(|r| self.row_text(r))
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl Perform for ScreenInner {
    fn print(&mut self, c: char) {
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.grid[self.cursor_row][self.cursor_col] = c;
            self.cursor_col += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    // Scroll up
                    self.grid.remove(0);
                    self.grid.push(vec![' '; self.cols]);
                    self.cursor_row = self.rows - 1;
                }
            }
            b'\r' => {
                self.cursor_col = 0;
            }
            b'\x08' if self.cursor_col > 0 => {
                self.cursor_col -= 1;
            } // backspace
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let p: Vec<u16> = params.iter().flat_map(|s| s.iter().copied()).collect();
        let p1 = p.first().copied().unwrap_or(1) as usize;
        let p2 = p.get(1).copied().unwrap_or(1) as usize;

        match action {
            'A' => {
                self.cursor_row = self.cursor_row.saturating_sub(p1);
            } // cursor up
            'B' => {
                self.cursor_row = (self.cursor_row + p1).min(self.rows - 1);
            } // cursor down
            'C' => {
                self.cursor_col = (self.cursor_col + p1).min(self.cols - 1);
            } // cursor right
            'D' => {
                self.cursor_col = self.cursor_col.saturating_sub(p1);
            } // cursor left
            'H' | 'f' => {
                // cursor position
                self.cursor_row = (p1.saturating_sub(1)).min(self.rows - 1);
                self.cursor_col = (p2.saturating_sub(1)).min(self.cols - 1);
            }
            'J' => {
                // erase display
                let mode = p.first().copied().unwrap_or(0);
                match mode {
                    0 => {
                        // erase below
                        self.clear_line(self.cursor_row);
                        for r in (self.cursor_row + 1)..self.rows {
                            self.clear_line(r);
                        }
                    }
                    1 => {
                        // erase above
                        for r in 0..self.cursor_row {
                            self.clear_line(r);
                        }
                    }
                    2 | 3 => {
                        // erase all
                        for r in 0..self.rows {
                            self.clear_line(r);
                        }
                    }
                    _ => {}
                }
            }
            'K' => {
                // erase line
                let mode = p.first().copied().unwrap_or(0);
                let row = self.cursor_row;
                if row < self.rows {
                    match mode {
                        0 => {
                            for c in self.cursor_col..self.cols {
                                self.grid[row][c] = ' ';
                            }
                        }
                        1 => {
                            for c in 0..=self.cursor_col.min(self.cols - 1) {
                                self.grid[row][c] = ' ';
                            }
                        }
                        2 => {
                            self.clear_line(row);
                        }
                        _ => {}
                    }
                }
            }
            'G' => {
                self.cursor_col = (p1.saturating_sub(1)).min(self.cols - 1);
            } // cursor horizontal absolute
            'd' => {
                self.cursor_row = (p1.saturating_sub(1)).min(self.rows - 1);
            } // cursor vertical absolute
            _ => {} // ignore SGR (m), mode sets (h/l), etc.
        }

        // DEC private mode set/reset: CSI ? Ps h / CSI ? Ps l
        // Track alternate screen buffer (used by nvim, vim, htop, less, etc.)
        if (action == 'h' || action == 'l') && _intermediates == [b'?'] {
            for &param in &p {
                if param == 1049 || param == 1047 || param == 47 {
                    self.alt_screen = action == 'h';
                    // On exit (l): clear grid so TUI residue doesn't pollute
                    // shell preview extraction. A real terminal restores the
                    // saved main buffer; we just clear since we don't need
                    // scrollback fidelity — the shell prompt redraws immediately.
                    if action == 'l' {
                        for r in 0..self.rows {
                            self.clear_line(r);
                        }
                        self.cursor_row = 0;
                        self.cursor_col = 0;
                    }
                }
            }
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

impl Default for VScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl VScreen {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            inner: ScreenInner::new(),
        }
    }

    /// Feed raw PTY data into the virtual screen
    pub fn feed(&mut self, data: &[u8]) {
        self.parser.advance(&mut self.inner, data);
    }

    /// Get all non-empty screen rows
    pub fn rows(&self) -> Vec<String> {
        self.inner.all_rows()
    }

    /// Get a specific row
    pub fn row(&self, n: usize) -> String {
        self.inner.row_text(n)
    }

    /// Returns true when the alternate screen buffer is active (TUI app running)
    pub fn is_alt_screen(&self) -> bool {
        self.inner.alt_screen
    }

    /// Extract the "last meaningful message" from the screen.
    /// Skips TUI chrome (status bars, prompts, separators, hints).
    pub fn extract_last_message(&self, agent: Option<&str>) -> Option<String> {
        let rows = self.rows();
        // Walk rows bottom-up, skip chrome, return first real content
        for row in rows.iter().rev() {
            let t = row.trim();
            if t.is_empty() || t.len() < 3 {
                continue;
            }
            if is_chrome(t, agent) {
                continue;
            }

            // Strip leading bullet/dot prefixes that agents use
            // Claude: "● text" or "• text"
            // Codex: "· text" or "• text"
            let cleaned = t
                .trim_start_matches('●')
                .trim_start_matches('•')
                .trim_start_matches('·')
                .trim_start_matches('›')
                .trim_start_matches('▸')
                .trim();

            if cleaned.is_empty() || cleaned.len() < 2 {
                continue;
            }

            let result = if cleaned.chars().count() > 60 {
                let end = cleaned
                    .char_indices()
                    .nth(57)
                    .map(|(i, _)| i)
                    .unwrap_or(cleaned.len());
                format!("{}…", &cleaned[..end])
            } else {
                cleaned.to_string()
            };
            return Some(result);
        }
        None
    }

    /// Detect agent state from screen content (for tools without OSC title)
    pub fn detect_state(&self, agent: Option<&str>) -> Option<String> {
        let rows = self.rows();

        // Strategy: scan screen for prompt indicators.
        // If the input prompt is visible, the agent is idle (waiting for user).
        // If a spinner/thinking indicator is visible, it's thinking.
        for row in rows.iter().rev() {
            let t = row.trim();
            let tl = t.to_lowercase();
            if t.is_empty() {
                continue;
            }

            // Claude: spinner chars = thinking, ❯ prompt = idle
            if agent == Some("claude") {
                if t.starts_with('✢')
                    || t.starts_with('✳')
                    || t.starts_with('✶')
                    || t.starts_with('✻')
                    || t.starts_with('✽')
                {
                    return Some("thinking".to_string());
                }
                if t.starts_with('❯') {
                    return Some("idle".to_string());
                }
            }

            // Kiro: status bar with "Kiro · auto" = idle, spinner ● with animation = thinking
            if agent == Some("kiro") {
                if tl.starts_with("kiro") && tl.contains("auto") {
                    return Some("idle".to_string());
                }
                // Kiro shows ● with animated text when thinking
                if t.starts_with('●') && (tl.contains("...") || tl.contains("…")) {
                    return Some("thinking".to_string());
                }
            }

            // Codex: > prompt visible = idle, "working" text = thinking
            if agent == Some("codex") {
                if tl.contains("working") {
                    return Some("thinking".to_string());
                }
                // Codex status bar with model info = idle
                if tl.contains("gpt-") && tl.contains("xhigh") {
                    return Some("idle".to_string());
                }
            }
        }
        None
    }
}

/// Returns true if the line is TUI chrome (not user-meaningful content)
fn is_chrome(line: &str, agent: Option<&str>) -> bool {
    let t = line.to_lowercase();
    let len = line.len();

    // === Universal chrome ===
    // Separator lines (all dashes/box chars)
    let alpha_count = line.chars().filter(|c| c.is_alphanumeric()).count();
    if alpha_count == 0 {
        return true;
    }

    // === Claude Code chrome ===
    if agent == Some("claude") || t.contains("claude") {
        if t.contains("context") && t.contains("usage") && t.contains("%") {
            return true;
        }
        if t.contains("mcp") && len < 20 {
            return true;
        }
        if t.contains("opus") && t.contains("context") {
            return true;
        }
        if t.contains("claude code v") {
            return true;
        }
        if t.contains("welcome back") {
            return true;
        }
        if t.contains("tips for getting started") {
            return true;
        }
        if t.contains("no recent activity") {
            return true;
        }
        if t.contains("recent activity") && len < 30 {
            return true;
        }
        if t.contains("imageinclipboard") || t.contains("ctrl+v") {
            return true;
        }
        if t.contains("/effort") || t.contains("xhigh") || t.contains("xlow") {
            return true;
        }
        // Trust prompt
        if t.contains("yes, i trust") || t.contains("no, exit") {
            return true;
        }
        if t.contains("accessing workspace") {
            return true;
        }
        if t.contains("quick safety check") {
            return true;
        }
        if t.contains("be able to read, edit") {
            return true;
        }
        if t.contains("security guide") {
            return true;
        }
        // User input (❯ prefix) — filter ALL ❯ lines, they're either empty prompt or user typing
        if line.trim_start().starts_with('❯') {
            return true;
        }
        // Thinking spinners (✢ Proofing…, ✳ Moonwalking…, etc.)
        if line.trim_start().starts_with('✢')
            || line.trim_start().starts_with('✳')
            || line.trim_start().starts_with('✶')
            || line.trim_start().starts_with('✻')
            || line.trim_start().starts_with('✽')
        {
            return true;
        }
        // Organization/path info
        if t.contains("organization") && t.contains("@") {
            return true;
        }
    }

    // === Kiro CLI chrome ===
    if agent == Some("kiro") || t.contains("kiro") {
        if t.contains("/copy to clipboard") {
            return true;
        }
        if t.contains("ask a question or describe a task") {
            return true;
        }
        if t.starts_with("kiro") && t.contains("auto") {
            return true;
        }
        if t.contains("credits:") && t.contains("time:") {
            return true;
        }
        if t.contains("mcp failure") {
            return true;
        }
        if t.contains("welcome to the new kiro") {
            return true;
        }
        if t.contains("thinking") && t.contains("esc to cancel") {
            return true;
        } // thinking indicator
        if t.contains("prefer the classic experience") {
            return true;
        }
        if t.contains("/tui to learn more") {
            return true;
        }
    }

    // === Codex chrome ===
    if agent == Some("codex") || t.contains("codex") || t.contains("openai") {
        if t.contains(">_ openai codex") {
            return true;
        } // banner
        if t.contains("model:") && t.contains("/model") {
            return true;
        }
        if t.contains("directory:") {
            return true;
        }
        if t.contains("/skills to list") {
            return true;
        }
        if t.contains("gpt-") && t.contains("xhigh") {
            return true;
        } // model status
        if t.starts_with("tip:") {
            return true;
        }
        if t.starts_with(">") && len < 5 {
            return true;
        } // empty prompt
    }

    // === Common chrome ===
    if t.contains("security guide") {
        return true;
    }
    if t.contains("enter to confirm") {
        return true;
    }
    if t.contains("initializing") && len < 30 {
        return true;
    }

    // === User input lines (prompt + typed text) ===
    // Claude: ❯ followed by user text (handled in Claude section above)
    // Codex: › followed by user text
    if line.trim_start().starts_with('›') {
        return true;
    }
    // Codex: > followed by user text (short lines only)
    if line.trim_start().starts_with('>') && !line.trim_start().starts_with(">>") && len < 80 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_print() {
        let mut vs = VScreen::new();
        vs.feed(b"Hello World");
        assert_eq!(vs.row(0), "Hello World");
    }

    #[test]
    fn test_newline() {
        let mut vs = VScreen::new();
        vs.feed(b"line1\r\nline2");
        assert_eq!(vs.row(0), "line1");
        assert_eq!(vs.row(1), "line2");
    }

    #[test]
    fn test_cursor_move_and_overwrite() {
        let mut vs = VScreen::new();
        vs.feed(b"ABCDE\x1b[1;1HXYZ"); // move to 1,1 and overwrite
        assert_eq!(vs.row(0), "XYZDE");
    }

    #[test]
    fn test_erase_display() {
        let mut vs = VScreen::new();
        vs.feed(b"Hello\r\nWorld\x1b[2J"); // clear screen
        assert!(vs.rows().is_empty());
    }

    #[test]
    fn test_claude_chrome_detection() {
        assert!(is_chrome(
            "Context ████ 3% │ Usage ████ 2% (1h 57m / 5h) │ ████ 11% (3d 18h / 7d)",
            Some("claude")
        ));
        assert!(is_chrome("❯", Some("claude")));
        assert!(is_chrome("◉ xhigh · /effort", Some("claude")));
        assert!(is_chrome("Tips for getting started", Some("claude")));
        assert!(!is_chrome("你好！有什么可以帮你的吗？", Some("claude")));
        assert!(!is_chrome(
            "● 你好。需要我帮你看代码、跑命令，还是处理别的事情？",
            Some("claude")
        ));
    }

    #[test]
    fn test_kiro_chrome_detection() {
        assert!(is_chrome("/copy to clipboard", Some("kiro")));
        assert!(is_chrome(
            "ask a question or describe a task ↵",
            Some("kiro")
        ));
        assert!(is_chrome("Kiro · auto · ◎ 4%", Some("kiro")));
        assert!(is_chrome("▸ Credits: 0.14 · Time: 4s", Some("kiro")));
        assert!(!is_chrome("你好！有什么可以帮你的吗？", Some("kiro")));
    }

    #[test]
    fn test_codex_chrome_detection() {
        assert!(is_chrome(">_ OpenAI Codex (v0.121.0)", Some("codex")));
        assert!(is_chrome(
            "model:     gpt-5.4 xhigh   /model to change",
            Some("codex")
        ));
        assert!(is_chrome(
            "gpt-5.4 xhigh · ~/my_project/chat_term_tests",
            Some("codex")
        ));
        assert!(is_chrome(
            "Tip: New Use /fast to enable our fastest inference",
            Some("codex")
        ));
        assert!(!is_chrome("收到，当前会话和工作区正常。", Some("codex")));
    }

    #[test]
    fn test_extract_last_message_claude() {
        let mut vs = VScreen::new();
        // Simulate Claude screen: banner, reply, status bar
        vs.feed(b"Claude Code v2.1.114\r\n");
        vs.feed(b"\xe2\x97\x8f \xe4\xbd\xa0\xe5\xa5\xbd\xef\xbc\x81\r\n"); // ● 你好！
        vs.feed(b"Context 3% | Usage 2%\r\n");
        vs.feed(b"\xe2\x9d\xaf \r\n"); // ❯
        let msg = vs.extract_last_message(Some("claude"));
        assert_eq!(msg, Some("你好！".to_string()));
    }

    #[test]
    fn test_alt_screen_tracking() {
        let mut vs = VScreen::new();
        assert!(!vs.is_alt_screen());

        // CSI ?1049h — enter alternate screen (nvim, vim, etc.)
        vs.feed(b"\x1b[?1049h");
        assert!(vs.is_alt_screen());

        // CSI ?1049l — leave alternate screen
        vs.feed(b"\x1b[?1049l");
        assert!(!vs.is_alt_screen());

        // CSI ?47h — legacy alternate screen sequence
        vs.feed(b"\x1b[?47h");
        assert!(vs.is_alt_screen());

        vs.feed(b"\x1b[?47l");
        assert!(!vs.is_alt_screen());

        // CSI ?1047h — xterm/ncurses variant
        vs.feed(b"\x1b[?1047h");
        assert!(vs.is_alt_screen());

        vs.feed(b"\x1b[?1047l");
        assert!(!vs.is_alt_screen());
    }

    #[test]
    fn test_alt_screen_exit_clears_grid() {
        let mut vs = VScreen::new();
        vs.feed(b"\x1b[?1049h");
        vs.feed(b"nvim residue line");
        assert_eq!(vs.row(0), "nvim residue line");

        // Exiting alt screen should clear the grid
        vs.feed(b"\x1b[?1049l");
        assert!(vs.rows().is_empty());
    }
}
