# ChatTerm Agent ANSI 捕获与分析

> English: see [`README.en.md`](README.en.md)。两份文件保持同步。

一个离线的 PTY 录制 + ANSI 覆盖率分析工具。用来：

- 研究一个新的 AI Coding Agent CLI 在终端里**到底写了什么字节**
- 对比不同 Agent 的 chrome（title / mouse / 同步更新 / 权限对话框等）
- 把 chatterm 对 Agent 的识别和渲染做成**有数据支撑**的而不是拍脑袋

本文档分三块：**调研结论** / **测试方法** / **新 Agent 适配指南**。

---

## 一、调研结论（2026-04-22 首轮）

同一套 prompt（"解释 PTY" + "写 fizzbuzz" + "建文件"）跑了三个 agent，差异大到可以说是**三种完全不同的渲染架构**。

### 1.1 三方对比表

| 维度                          | Claude Code             | Codex CLI               | Kiro CLI                  |
| ----------------------------- | ----------------------- | ----------------------- | ------------------------- |
| 版本                          | 2.1.117                 | codex-cli 0.122.0       | kiro-cli 2.0.1            |
| 同 prompt 字节量              | 32 KB                   | **5.5 KB**              | **97 KB**                 |
| 渲染模式                      | 增量光标派              | 标准保守派              | 全屏重绘派                |
| OSC 0/2 title                 | 有（带状态）            | 有（cwd + 思考 spinner）| **完全不发**              |
| 同步更新 `?2026`              | 0                       | 7 对                    | **214 对（每帧都包）**    |
| 光标切换 `?25`                | 3 平衡                  | 6↑/1↓                   | 0↑/227↓（**退出不恢复**） |
| Mouse tracking                | 全 4 种（1000/2/3/6）   | 无                      | 无                        |
| Focus events `?1004`          | 有                      | 有                      | 无                        |
| Color-scheme notify `?2031`   | 有                      | 无                      | 无                        |
| Kitty keyboard `CSI u`        | 无                      | **有**（`\e[>7u`）      | 无                        |
| DECSTBM 滚动区                | 无                      | **4 次**                | 无                        |
| 全屏清 `\e[2J`                | 偶发                    | 偶发                    | **105 次（per-frame）**   |
| 词分隔符                      | **`\e[1C`（不是空格）** | 空格                    | 空格                      |
| OSC 颜色查询                  | 无                      | `\e]10;?;\a` 查 fg      | `\e]11;?;\a` 查 bg        |
| 其他协议                      | OSC 9（iTerm growl）    | `\e[6n` DSR×2           | `\e[6n` DSR×1             |
| 思考标记                      | title 前缀 braille 动画 | title 前缀 braille 动画 | body 文本 `⠀ Thinking...` |
| **思考 spinner 帧数**         | title `⠂⠐` **2 帧** blink | title `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` **10 帧**整圈 | body `⠀⠁⠉⠙⠹⢹⣹⣽⣿` **9 帧**累积填充 |
| **权限对话框**                | 无 box-drawing            | trust dialog（启动时）   | 有 box-drawing                       |
| **"一键通过"模式**            | option 2 触发 `⏵⏵ accept edits on` | *未确认*        | option 2 触发（chrome 待测）         |

### 1.2 Claude Code 的关键细节

**Title 协议**是分层的：

| 状态              | 示例                                                  |
| ----------------- | ----------------------------------------------------- |
| 启动时 idle       | `\e]0;✳ Claude Code\a`                                |
| 正在思考（未开工）| `\e]0;⠂ Claude Code\a` / `\e]0;⠐ Claude Code\a`（**仅 2 帧** blink，不是完整 braille 环）|
| 正在执行任务      | `\e]0;⠂ Read hello.txt and search for TODO\a`         |
| 任务完成          | `\e]0;✳ Read hello.txt and search for TODO\a`         |

**⚠ 陷阱**：完成任务后 title **不会**恢复成 `✳ Claude Code`，而是永远保留上一个任务名做前缀。所以 "title == `✳ Claude Code`" 只能判别**首次启动后**的 idle，不能判别会话中途的 idle。Agent state 检测必须**组合**以下信号：

- title 前缀字符（`✳` 稳定 / braille 循环 = 思考中）
- body 出现 `⏺` + `Interrupted · What should Claude do instead?` = 等待用户重定向
- body 出现 `Do you want to proceed?` / `Do you want to make this edit` = 等待权限确认
- 字节 idle 阈值兜底

**`\e[1C` 做词分隔**：整个 dialog 里**不是用空格分词**，而是用 CUF-1（光标前进一格）。例如权限对话框的原始字节：

```
Yes,\e[1Cand\e[1Calways\e[1Callow\e[1Caccess\e[1Cto\e[1C<bold>chatterm-capture-sandbox<end-bold>/\e[1Cfrom\e[1Cthis\e[1Cproject
```

对 chatterm 的直接影响：**sidebar preview 抽取**时如果简单把 CUF 序列剥掉，相邻词会粘在一起（`Yes,andalwaysallowaccess...`）。正确做法是把 CUF-N 等价成 N 个空格插入到屏幕 grid 里，或者在 preview 提取路径上用虚拟屏幕（`vscreen.rs`）的二维表示来读取。

**权限对话框两种变体**：

```
# Bash 类（touch / ls / rm）
Do you want to proceed?
❯ 1. Yes
  2. Yes, and always allow access to <cwd>/ from this project
  3. No
Esc to cancel · Tab to amend · ctrl+e to explain

# Edit 类（含完整 diff hunk 在上方）
Do you want to make this edit to <file>?
❯ 1. Yes
  2. Yes, allow all edits during this session (shift+tab)
  3. No
```

均**无 box-drawing 字符**，靠 `\e[1C` + truecolor SGR + 行尾空格 padding 拼视觉效果。选中项用真彩 lavender `rgb(177,185,249)` 的 `❯` + 同色文本；未选项全灰 `rgb(153,153,153)`。

**Edit diff 用 Monokai 主题 + 背景色区分增删**：

- 添加行：背景 `rgb(4,71,0)`（深绿）+ 前景 `rgb(80,200,80)`（亮绿）
- 删除行：背景 `rgb(61,1,0)`（深红）+ 前景 `rgb(220,90,90)`（暗红）
- **增删两边都有语法高亮**（同一套 Monokai 前景色）—— 所以"有没有代码高亮"不能区分 diff 方向

**"一键通过"模式**（第二轮补录，选项 2 的副作用）：按权限对话框的 `2` 之后，Claude 进入会话级 trust 状态，后续**所有 tool 调用跳过权限对话框**（实测 `touch` 放行后，`rm` 也直接跑）。底部状态栏新增持久指示器：

```
⏵⏵ accept edits on (shift+tab to cycle)
```

该模式用 `shift+tab` 可循环切换若干档位（目前只实测到 `accept edits on` 这一档）。chatterm 当前**完全没捕获**这个信号。

**Title 的 braille 只有 2 帧**（第二轮确认）：跨 5 个 case、~250 KB 数据，Claude title 只用 `⠂` (U+2802) 和 `⠐` (U+2810) 交替 blink，**不是**完整 8 帧 braille 环。body 里的 spinner 字符反而更多（`✢ ✳ ✶ ✻ ✽ ·` 配合 `Roosting…` / `Reticulating…` / `Gallivanting…` 等随机 gerund）。

### 1.3 Codex CLI 的关键细节

- **Title = cwd 名**。思考时前缀 braille 字符，**整圈 10 帧**：`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`（顺时针点阵）。完整例：`\e]0;⠦ chatterm-capture-sandbox\a`。
- **首次启动有 trust dialog**：`Do you trust the contents of this directory?` → `1. Yes, continue / 2. No, quit`。不按 Enter 确认就无法进入正常会话（fixture 容易忘这一步）。
- **MCP 全部加载完才接受 prompt**（第二轮发现）：codex 启动会并行拉起配置里所有 MCP 服务器，body 显示 `Starting MCP servers (N/M): <name>`。这之前任何 prompt 都会被塞进 input buffer 等待（`tab to queue message`），**`\r` 不提交**。fixture 的 ready regex 不能只认 trust dialog 通过，还要等 `Starting MCP servers` 消失或状态栏 `gpt-N.N xhigh · <cwd>` 稳定。
- **使用 kitty keyboard protocol**：`\e[>7u`（push flag）/ `\e[<1u`（pop flag）。这是 xterm modifyOtherKeys 的替代方案，让 Ctrl+Shift+组合键能被明确区分。终端**必须**支持才能正确收键。
- **使用 DECSTBM 滚动区**：`\e[1;40r` 把滚动区限制在固定行范围，让输出滚动不影响底部状态栏。这是经典 TUI 做法，claude 和 kiro 都没用。
- **字节量最省**：5.5 KB 做完同样 3 个 prompt。说明 codex 的重绘策略很精细，只更新变化的区域。
- **Prompt 提交必须走 bracketed paste**（第二轮踩坑总结）：Codex 启动就 push Kitty flag 7，keystroke 路径下 `\r` 被 disambiguate 成 `\e[13u`，而 Codex 的 crossterm 在 flag 7 下**不再**接受裸 `\r`；`\e[13u` / `\e[13;1u` / `\e[13;1:1u` 等各种 Kitty 编码我都试过也不提交。**唯一工作的编码**是把 prompt 包进 bracketed paste：`\e[200~<prompt>\e[201~\r` —— crossterm 的 paste 事件通道和 keystroke 正交，paste-end 之后的 `\r` 作为独立 Enter 被接收。所以本工具 codex fixture 的 `send` 必须手动包裹，`codex-basic.toml` 里有模板。

### 1.4 Kiro CLI 的关键细节

- **完全不发 OSC title**。chatterm 的 `agents.json` 如果只认 title 就永远识别不出 kiro。必须用 argv 匹配（`command` 里含 `kiro-cli`）或 body 文本匹配（`Thinking... (esc to cancel)`）来补。
- **`?2026` 同步更新每帧包裹**：214 对 set/reset 在 106 秒内 ≈ 2 fps。**完全依赖终端支持**（iTerm2/kitty/WezTerm 支持，基础 xterm/gnome-terminal 不支持）；不支持的终端上会看到中间态 tearing。
- **全屏重绘派**：每帧 `\e[2J\e[H` 清屏 + 重画整屏。对性能友好（不用做局部 diff），但字节量是 claude 的 3 倍。
- **退出 Bug**：`?25`（光标可见性）reset 了 227 次、set 了 0 次。正常退出后终端可能处于"光标隐藏"状态，下一条命令提示符看不到光标。这不是 chatterm 能修的，但**我们的渲染器如果自己缓存过 `?25` 状态**，可以在 agent 进程结束时强制 `\e[?25h` 兜底。
- **不发 mouse / focus / color-scheme 事件**。假设的是"终端就是终端"，不做现代终端能力协商。
- **body spinner 是 9 帧累积填充**（第二轮枚举）：`⠀ ⠁ ⠉ ⠙ ⠹ ⢹ ⣹ ⣽ ⣿`（每帧多几个点，不循环 — 满了之后推测会重置）。body 行形如 `⠀ Thinking... (esc to cancel)`，`⠀` (U+2800 空 braille) 是起始帧，不是普通空格。
- **有权限对话框**（第一轮漏测，第二轮补到）—— 见下方专门段落。

**权限对话框（第二轮实拍，纠正第一轮"Kiro 无权限 dialog"的说法）：**

```
────────────────────────────────────────
 write requires approval               ← verb 可变：write / shell / read / ...
 ❯ Yes, single permission               ← 单次通过
   Trust, always allow in this session   ← 会话 trust（= Claude 选项 2 的等价物）
   No (Tab to edit)                      ← 拒绝 + 可改 prompt 重试
────────────────────────────────────────
 ESC to close | Tab to edit
```

- **带 box-drawing**（`─`），和 Claude 权限对话框风格相反
- 稳定主规则：正则 `\w+ requires approval`（`write` / `shell` 已实测，其他 verb 推测存在）
- 选项 3 的 `Tab to edit` 是 Kiro 特有的 UX —— 用户不满 prompt 可以**直接进编辑态**改完再提交，不用退出对话框

### 1.5 对 chatterm 实现的直接启发

| 发现                                   | 建议 action                                                                     |
| -------------------------------------- | ------------------------------------------------------------------------------- |
| `\e[1C` 做词分隔                        | preview 提取走 `vscreen` 的 2D grid，不做字节级 chrome stripping                  |
| Claude title 只首次是 idle             | state detector 组合 title 前缀 + body 文本 + 字节 idle，不能只看 title            |
| Kiro 无 title                          | `agents.json` 增加 argv 匹配路径和 body-text fallback（`Thinking... (esc to cancel)`）|
| `?2026` 在 kiro 上每帧包裹             | `vscreen` 至少识别这个模式（可以不实现真正的双缓冲，但要知道这是 kiro 的必备）    |
| Kitty keyboard `CSI u` 是 codex 键盘协议 | chrome stripper 不能把 `\e[>Nu` / `\e[<Nu` 当 SCORC/SCOSC 剥掉                   |
| 权限对话框无 box-drawing                | 识别权限弹窗用文本特征：`Do you want to proceed?` / `Do you want to make this edit` |
| Claude Edit diff 用背景色区分增删        | diff 区域识别用 `\e[48;2;...m` 背景色 pattern，不要靠字符 `+/-`                   |
| Kiro 退出留 `?25` 不恢复                | chatterm 在 PTY EOF 时主动发 `\e[?25h` 兜底                                     |
| Claude title 只 2 帧 blink               | ✅ `agents.json` 的 thinking 前缀已改成 `["⠂","⠐"]`（cb8307d）                    |
| Codex title 10 帧未配                    | ✅ `agents.json` 已补 `osc_title_prefix: ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]`（cb8307d）|
| Kiro body spinner 可作快触发             | 在 `Thinking\.\.\.` 基础上追加动画字符 `⠀/⠁/⠉/⠙/⠹/⢹/⣹/⣽/⣿` 触发器（未做）         |
| Kiro 权限对话框有 box-drawing 变体         | ✅ 识别规则 `\w+ requires approval` 已落地 `agents.json` asking（cb8307d）        |
| Claude "accept edits on" 模式隐形         | 新状态（或 agent 徽章）：匹配 `⏵⏵ accept edits on`，告知用户"无需确认模式"开启（未做）|
| Codex prompt 必须 bracketed-paste 提交    | ✅ fixture 里用 `\e[200~...\e[201~\r` 包裹；已在 codex-basic.toml 提供模板（a0c7624）|

### 1.6 Asking 状态（已落地：cb8307d + d98df25 + 96fedbe + a0c7624）

现有 `StateDef` 原本只有 `thinking / idle`。权限对话框弹出时 Claude title 仍是 `✳ <task>`（命中 idle 规则），实际用户被阻塞 —— 这是**现行规则的 bug**，不只是缺 feature。`cb8307d` 落地了非必填字段 `asking`，优先级 `asking > thinking > idle`；`96fedbe` 把 Sidebar 的 asking 状态点做成红色快脉动以视觉强调。

最终落地的 regex（三家都已 ship 并实拍验证命中）：

```jsonc
// Claude
"asking": {
  "screen_regex": [
    "Do you want to proceed\\?",             // Bash 类权限
    "Do you want to make this edit to",      // Edit 类权限
    "What should Claude do instead\\?"       // Deny / Ctrl-C 后的重定向等待
  ]
}

// Kiro
"asking": {
  "screen_regex": [
    "requires approval",                     // 主规则，verb 不限（write / shell / web_fetch 都 cover）
    "Yes, single permission",                // 备份规则 1
    "Trust, always allow in this session"    // 备份规则 2
  ]
}

// Codex（d98df25 基于用户截图 + a0c7624 harness 实拍双重验证）
"asking": {
  "screen_regex": [
    "Do you trust the contents of this directory",  // 启动 trust dialog
    "Would you like to run the following command",  // 运行时 sandbox 权限
    "No, and tell Codex what to do differently"     // 选项 3 兜底
  ]
}
```

Hook API 的 `"ask"` 事件（现 Codex / Claude hooks 支持）同步从"映射到 idle"改成"映射到 asking"（cb8307d）。

**未迁移的 chrome 规则（后续清单）**：现有 `chrome[]` 里仍有一批 asking chrome 的构成文本被当 chrome 剥掉，导致 asking 状态下 preview 看不到对话框内容：

- Claude: `"yes, i trust|no, exit"`、`"enter to confirm"`、`"quick safety check"`、`"be able to read, edit"`、`"security guide"`

这些应该从 chrome 迁移到 asking.screen_regex（或者让 preview 抽取路径感知 asking 状态，不剥 chrome）。目前**没做**，需要配合 `pty.rs` preview 路径一起改 —— 见"后续方向"。

---

## 二、测试方法

### 2.1 架构

```
fixture.toml  ──►  recorder  ──►  PTY（spawn agent）──►  reader thread
                      │                                       │
                      │                                       ▼
                      │                                  raw.bin（字节精确）
                      │                                  cast.json（asciinema v2 可回放）
                      │                                  stream.ndjson（每 chunk 带时间戳+hex）
                      ▼
                   analyzer（vte::Parser + Perform trait）
                      │
                      ▼
                   coverage.json（分类计数、first_offset、样例）
                      │
                      ▼
                   report.html（跨 case 热力矩阵 + drill-down）
```

关键设计：

1. **byte-exact 与 UTF-8 分开**：`raw.bin` 是 PTY master 端 `read()` 的原始字节，任何分析、回归、对比都以它为准。`cast.json` 走 `String::from_utf8_lossy`，仅供 `asciinema play` 回放。
2. **分析器共用 chatterm 自己的 vte crate**（0.15），保证和 `src-tauri/src/vscreen.rs` 的解析语义一致——分析器看到的和运行时看到的是同一套状态机。
3. **`report` 只读 `coverage.json`**，不重跑 fixture。真 agent 捕获要烧 token、耗时、且不可确定重现，所以"跑"和"看报告"必须分开。

### 2.2 目录结构

```
capture/
├── Cargo.toml
├── src/
│   ├── main.rs       # CLI 分发
│   ├── fixture.rs    # TOML schema
│   ├── recorder.rs   # PTY 驱动 + 多路 tee
│   ├── analyzer.rs   # vte 分类器
│   └── report.rs     # HTML 报告
├── fixtures/
│   ├── bash-*.toml           # 不需要 agent 的基线 fixture（SGR、OSC、mouse 等）
│   └── agents/
│       ├── claude-*.toml     # 真 agent fixture（需手动触发）
│       ├── codex-*.toml
│       └── kiro-*.toml
└── artifacts/                # 捕获产物，每 case 一个目录
    └── <agent>/<case>/
        ├── raw.bin
        ├── cast.json
        ├── stream.ndjson
        └── coverage.json
```

**重要**：`capture coverage fixtures/` 只扫**顶层** `.toml` 文件，不递归进 `agents/` 子目录。这是故意的——bash fixture 毫秒级免费跑，agent fixture 动辄几分钟+烧 token，不能混在一起 sweep。

### 2.3 命令

```bash
# 跑单个 fixture（自动分析并输出 coverage 摘要）
cargo run -- run fixtures/bash-sgr.toml
cargo run -- run fixtures/agents/claude-permission.toml

# 批跑一个目录下的所有 fixture
cargo run -- coverage fixtures/           # bash 基线全跑
cargo run -- coverage fixtures/agents/    # 只跑 agent（显式）

# 只分析已有的 raw.bin（不重跑）
cargo run -- analyze artifacts/claude/tools/raw.bin

# 生成整合 HTML 报告
cargo run -- report artifacts/ --out artifacts/report.html
```

### 2.4 Fixture schema

```toml
agent = "codex"                  # 标识符，产物会落到 artifacts/<agent>/
case = "permission-approve"      # 同一 agent 下的子目录
command = ["codex"]              # 可选，默认 $SHELL -l
cwd = "/tmp/sandbox"             # 可选，工作目录（隔离副作用用）
cols = 140
rows = 40
idle_ms_default = 8000           # wait_idle 的默认窗口
timeout_ms = 240000              # 单次 run 的硬超时
env = { NO_COLOR = "0" }         # 可选附加环境变量

[[step]]
kind = "wait_regex"              # 匹配累积字节中的正则
pattern = "Do you want"          # 注意：\xNN 字节字面量已自动启用 (?-u)
timeout_ms = 90000

[[step]]
kind = "send"                    # 往 PTY 写字节
data = "1<CR>"                   # 支持命名键 token（见下文 §2.4.1）

[[step]]
kind = "wait_idle"               # 字节静默 ms 则继续
ms = 8000

[[step]]
kind = "resize"                  # 动态 resize 测试
cols = 80
rows = 24

[[step]]
kind = "sleep"                   # 无条件等待
ms = 1000
```

**陷阱**：

- TOML basic string **不允许**字面 `\x00-\x1F`（除 `\t`）。**首选方案**是下面的命名键 token；如果非要裸 unicode 写 ESC 用 `"\u001B"`、Ctrl-C 用 `"\u0003"`、Ctrl-D 用 `"\u0004"`。
- `wait_regex` 用 `regex::bytes` + 自动 `(?-u)` 前缀做**字节级**匹配，所以 `\\xe2\\x9c\\xb3` 能正确命中 `✳` 的 UTF-8 三字节。
- `send.data` **不做**二次 escape（除命名键 token 外）。想发 ESC 让 shell 的 printf 解释，就写 `"\\e[31m"`（TOML 解析后是 `\e[31m` 两字符）；想发真 ESC 字节，用 `"<ESC>[31m"`。

#### 2.4.1 命名键 token（推荐）

`send.data` 支持下列尖括号包裹的命名键 token，会在发送前展开成对应字节。这些 token 在**自然语言和 shell 文本中不会出现**，所以不会误伤正常 prompt：

| Token   | 字节  | 语义                    |
| ------- | ------- | ----------------------- |
| `<ESC>` | 0x1B    | ESC / 任何 CSI 起始字节 |
| `<C-c>` | 0x03    | Ctrl-C（中断）          |
| `<C-d>` | 0x04    | Ctrl-D（EOT）           |
| `<CR>`  | 0x0D    | Carriage Return         |
| `<LF>`  | 0x0A    | Line Feed               |
| `<TAB>` | 0x09    | Tab                     |
| `<BS>`  | 0x08    | Backspace               |

示例：关 dialog `<ESC>`、中断 `<C-c>`、exit TUI `<C-d>`、主动换行 `<LF>`。

#### 2.4.2 Codex 专用：bracketed-paste 提交

**Codex 启动就 push Kitty keyboard flag 7**，keystroke 路径下裸 `\r` 不会 submit（详见 §1.3）。所以 Codex fixture 里每条 prompt 都要包进 bracketed paste：

```toml
[[step]]
kind = "send"
data = "<ESC>[200~say hi in one word<ESC>[201~\r"
```

Crossterm 的 paste 事件通道和 keystroke 正交，paste-end 后的 `\r` 作为独立 Enter 被处理。Claude / Kiro 不需要这么干，`"say hi\r"` 直接可用。`codex-basic.toml` 和 `codex-permission-matrix.toml` 是两个参考模板。


### 2.5 ANSI 类别分桶

`analyzer.rs` 把每个 vte 事件分到这 20 个桶之一：

| 桶                        | 对应事件                                        |
| ------------------------- | ----------------------------------------------- |
| `print`                   | 可打印字符                                      |
| `c0`                      | C0 控制（BEL/BS/HT/LF/CR/FF...）                 |
| `csi_sgr`                 | CSI `m`（颜色、样式）                            |
| `csi_cursor`              | CSI `A/B/C/D/E/F/G/H/f/d/s/u`                   |
| `csi_erase`               | CSI `J/K/X`（ED / EL / ECH）                     |
| `csi_edit`                | CSI `@/L/M/P`（ICH / IL / DL / DCH）             |
| `csi_mode_set`            | CSI `h`（DEC / ANSI mode set）                   |
| `csi_mode_reset`          | CSI `l`                                         |
| `csi_scroll`              | CSI `r/S/T`（DECSTBM / SU / SD）                 |
| `csi_report`              | CSI `n/c`（DSR / DA）                            |
| `csi_other`               | CSI 其他 final byte（TBC/CHT/CBT 等）            |
| `osc_title`               | OSC 0/1/2                                       |
| `osc_color`               | OSC 4/10/11/12/17/19/104/110/111                |
| `osc_cwd`                 | OSC 7                                           |
| `osc_hyperlink`           | OSC 8                                           |
| `osc_clipboard`           | OSC 52                                          |
| `osc_shell_integration`   | OSC 133（iTerm/VSCode prompt marks）/ 633 / 697  |
| `osc_other`               | 其他 OSC（本次跑到了 OSC 9 growl）               |
| `dcs`                     | DCS（sixel、tmux passthrough 等）                |
| `esc_other`               | 单字符 ESC（DECSC `\e7` / DECRC `\e8` 等）       |

同时有三张横切表：`decset_modes`（每个私有模式号的 set/reset 计数）、`osc_codes`（每个 OSC 码的次数）、`sgr_attrs`（SGR 属性号的次数，**注意**：目前对 `\e[38;2;R;G;Bm` 的 RGB 分量也会被当单独 SGR 码计数，是个已知小 bug）。

---

## 三、新 Agent CLI 适配指南

加一个新 agent 的完整流程。示例以虚构的 `acme-cli` 为例。

### Step 1：确认它是交互式 TUI

```bash
acme-cli --help          # 看有没有 interactive / chat 子命令
which acme-cli           # 确认路径存在
acme-cli                 # 直接跑，观察：是立刻退出还是进 TUI
```

如果是 IDE launcher 类（例如 `kiro` 本身 = IDE 启动器、`code` = VSCode），**不属于本工具范围**。必须是"进程在前台跑、读 stdin 写 stdout"的交互式 CLI。

### Step 2：写一个最小启动 fixture（只 wait_idle）

目的是**让它跑起来**，不假设任何 chrome。

```toml
# fixtures/agents/acme-basic.toml
agent = "acme"
case = "basic"
command = ["acme-cli"]
cwd = "/tmp/chatterm-capture-sandbox"
cols = 140
rows = 40
idle_ms_default = 8000
timeout_ms = 240000

[[step]]
kind = "wait_idle"
ms = 4000

[[step]]
kind = "send"
data = "hello\r"

[[step]]
kind = "wait_idle"
ms = 20000

[[step]]
kind = "send"
data = ""            # Ctrl-D 兜底退出
```

跑：

```bash
cargo run -- run fixtures/agents/acme-basic.toml
```

**常见启动问题**：

| 症状                                      | 原因                                              | 对策                                         |
| ---------------------------------------- | ------------------------------------------------- | -------------------------------------------- |
| `write: Input/output error (os error 5)` | agent 进程已退出（banner 完事就结束 / 需要参数）    | 看 `raw.bin` 最后的 plain text，大概率是用法错 |
| 字节量很小，无可见交互                     | agent 需要登录（`*-cli login` 或浏览器 OAuth）       | 先登录再跑                                    |
| 首次启动卡住                              | 有启动 dialog（trust / accept TOS）               | 加一步 `send = "\r"` 先确认                    |
| PTY 无输出                                | agent 检测到非 tty 或小屏幕退化为非交互             | 检查 cols/rows，调大                          |

### Step 3：分析 `raw.bin` 找 chrome 标记

跑完后关键工具：

```bash
# 抽所有 OSC 0 title 变化
python3 -c "
import re
from itertools import groupby
data = open('artifacts/acme/basic/raw.bin','rb').read()
titles = re.findall(rb'\x1b\]0;([^\x07]*)\x07', data)
for t in [k.decode(errors='replace') for k,_ in groupby(titles)]:
    print(repr(t))
"

# 抽所有 plain text（剥 CSI/OSC）
python3 -c "
import re
data = open('artifacts/acme/basic/raw.bin','rb').read()
s = re.sub(rb'\x1b\[[0-9;?]*[a-zA-Z]', b'', data)
s = re.sub(rb'\x1b\][^\x07]*\x07', b'', s)
print(s.decode(errors='replace'))
" | less

# 看 coverage 拿到类别分布
cat artifacts/acme/basic/coverage.json | jq '.categories | keys'
cat artifacts/acme/basic/coverage.json | jq '.decset_modes'

# 用 asciinema 肉眼回放
asciinema play artifacts/acme/basic/cast.json
```

重点关注：

- **OSC title 协议**：是不是有？随状态变化吗？前缀有 spinner 动画吗？——这决定能不能用 title 做 state 检测
- **`?2026` 是否出现**：如果有，说明 agent 依赖同步更新，chatterm 渲染器要相应处理
- **权限/确认 dialog 的文本特征**：找一句稳定的、不会随 prompt 变化的句子（如 `Do you want to proceed?`）
- **思考/等待标记**：不一定在 title，可能在 body（如 kiro 的 `Thinking... (esc to cancel)`）
- **Kitty keyboard flag push（`\e[>Nu`）**：如果启动字节流里有这个序列（如 codex 的 `\e[>7u`），那 agent 多半用了 crossterm/ratatui 且启用了 progressive enhancement —— **裸 `\r` 可能不被识别为 Enter**。首次写 prompt 步骤没看到 title 动起来时八成是这个。对策：把 prompt 包进 bracketed paste（`<ESC>[200~...<ESC>[201~\r`），见 §2.4.2。

### Step 4：用 `wait_regex` 锁定关键跃迁

wait_idle 是底线，但真正稳的 fixture 要用 wait_regex 对准状态跃迁。例如：

```toml
# 等 agent 启动完成
[[step]]
kind = "wait_regex"
pattern = "\\xe2\\x9c\\xb3 Acme"      # ✳ Acme —— 启动 idle 标记
timeout_ms = 20000

# 等它进入思考
[[step]]
kind = "wait_regex"
pattern = "Thinking\\.\\.\\."
timeout_ms = 10000

# 等权限对话框出现
[[step]]
kind = "wait_regex"
pattern = "Do you want"
timeout_ms = 90000
```

**经验法则**：

- 真 agent 每次网络/思考延迟差异大，**超时给到 60-90 秒**才稳
- 一个 regex 尽量短且唯一。越长的句子越容易被 SGR/CUP 打断
- 字节转义 `\\xNN` 是最可靠的——字面 unicode 字符会被 TOML 转 UTF-8 后和 raw 流自然对齐，但复杂字符不如 hex 明确
- 匹配成功后 cursor 会前移，同一个 pattern 连续多次 wait 会依次命中多次出现

### Step 5：覆盖不同工具 / 分支

基本 fixture 跑通后，按 chatterm 需要验证的场景拆分：

```
fixtures/agents/
├── acme-basic.toml        # 冒烟：能启动、能对话、能退出
├── acme-permission.toml   # 权限对话框 approve 分支
├── acme-deny.toml         # 权限对话框 deny 分支
├── acme-tools.toml        # Read / Write / Grep 等工具调用
├── acme-interrupt.toml    # Ctrl-C 中断路径
└── acme-long-output.toml  # 长输出滚动（测 scroll region / alt-screen 行为）
```

每个 fixture 一个单一意图，方便出问题时快速定位是哪个场景的 chrome 变了。

### Step 6：把发现更新到 `src-tauri/agents.json`

最终目的是让 chatterm 识别新 agent。按本文档 §1.5 的对照表，需要至少提供：

- **argv 匹配**：`command` 包含 `acme-cli` → 识别为 acme
- **title 匹配**（如果有）：正则匹配启动/思考/idle title
- **state 规则**：每个 state（idle / thinking / waiting-confirm / awaiting-input）的触发特征
- **chrome 过滤**：哪些区域属于 agent 自绘的 chrome，preview 提取时应该跳过

### Step 7：回归基线

每次 agent 升版，重跑 fixture、diff `coverage.json`，**类别计数变化 >20% 或出现新类别**就说明 agent 的渲染行为变了，chatterm 侧可能需要跟进。

```bash
# 简单 diff（结构粒度）
diff <(jq -S '.categories | keys' artifacts/acme/basic/coverage.json) \
     <(jq -S '.categories | keys' artifacts/acme/basic-new/coverage.json)

# 计数粒度（主观阈值）
jq -r '.categories | to_entries | .[] | "\(.key) \(.value.count)"' \
  artifacts/acme/basic/coverage.json
```

未来会加一个 `capture diff` 子命令做这个（目前手工做）。

---

## 四、已知问题 / 后续方向

- `sgr_attrs` 把 truecolor 的 RGB 分量也当作 SGR 属性号计数，数字偏大。修法：遇到 38/48 后跳过后续颜色子参数。
- `csi_cursor` 桶混入了 Kitty keyboard protocol 的 `\e[>Nu` / `\e[<Nu`（final byte `u`），应该独立成 `csi_keyboard` 桶。
- 还没有 `capture diff` / `capture baseline` 子命令做回归断言。手工 `jq` 够用但繁琐。
- `cast.json` 走 `String::from_utf8_lossy`，遇到跨 UTF-8 边界分块的极端情况可能显示 `�`。回放演示够用，但不应作为分析源。
- Agent fixture 会产生真实副作用（在 cwd 建文件、改文件、调 API）。务必跑在一次性沙箱目录。
- Codex fixture 需要手动给每条 prompt 包 bracketed paste（`<ESC>[200~...<ESC>[201~\r`）。更便利的做法是让 recorder 识别 `agent = "codex"` 时自动包裹，但目前没做 —— 保持显式让规则可见。
- `chrome[]` 里仍有若干 trust/安全对话框文本被剥掉了（见 §1.6 末尾），asking 状态下 preview 显示会落空。需要配合 `pty.rs` 的 preview 路径一起改（让 asking 状态不剥 chrome，或者把这些 pattern 迁到 asking.screen_regex）。
