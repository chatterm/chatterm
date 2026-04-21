# ChatTerm

<p align="center">
  <img src="design/logo/v4/1C_bubble_blue_256.png" width="128" alt="ChatTerm">
</p>

<p align="center">
  <strong>为 AI 编程而生。</strong><br>
  IM 风格的终端工作区，像管理聊天对话一样管理终端会话。
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="design/chatterm-preview.png" alt="ChatTerm 截图">
</p>

## 为什么需要 ChatTerm？

同时运行多个 AI 编程 Agent（Claude Code、Kiro CLI、Codex）、SSH、构建、日志，传统终端的痛点：

- Tab/窗口太多，切换成本高
- 不知道哪个会话有新输出
- AI Agent 会话和 Shell、日志、构建混在一起
- 会话恢复能力弱，重启后上下文丢失
- 缺少面向 AI 编程的交互设计

ChatTerm 在真实终端之上提供 **IM 风格的会话管理层**。

## 功能特性

- **IM 风格侧边栏** — 会话以聊天对话形式展示，带头像、预览、未读标记
- **Agent 自动识别** — 识别 Claude Code、Kiro CLI、Codex，自动更新头像和状态
- **实时状态检测** — 通过 vscreen 模式匹配检测 thinking/idle 状态
- **Hook 驱动预览** — 通过 Named Pipe (FIFO) IPC 获取 Agent 回复预览，无需屏幕刮取
- **主题系统** — 内置 ChatTerm / VS Code Dark / Dark+，macOS 可导入 Terminal 主题
- **会话持久化** — 重启后恢复会话列表，Agent 会话支持 `--resume` 恢复
- **⌘K 搜索** — 按名称、工作目录或输出快速搜索会话
- **Shell 预览** — 显示 Shell 会话的最后命令和工作目录

## 技术栈

- **前端**: React 19 + TypeScript + Vite 7 + xterm.js
- **后端**: Rust + Tauri 2 + portable-pty
- **IPC**: Named Pipe (FIFO) 用于 hook → 应用通信
- **主题**: 可配置主题；macOS 支持导入 Terminal `.terminal` 配置

## 安装

ChatTerm 本体只是一个二进制；侧边栏的回复预览和思考/空闲指示还需要额外配**Agent Hooks**。先看下一节"hook 工作原理"，再按场景选一种安装方式——每种方式下面都把安装命令和对应的 hook 配置命令写在一起了。

### Agent Hooks 工作原理

Claude Code、Kiro CLI、Codex 都支持在 `Stop`、`PreToolUse`、`PostToolUse` 等事件上挂用户自定义命令。ChatTerm 就是靠这些 hook 来更新侧边栏的预览和状态点。

`setup-hooks.sh` 做两件事：

1. 把一个 Python hook 脚本写到稳定路径 `~/.chatterm/hook.sh`。它从 stdin 读事件 JSON，往 `~/.chatterm/hook.pipe` FIFO 写一条短消息，ChatTerm 的 Rust 后端读管道更新 UI。写 FIFO 用 `O_NONBLOCK`，**ChatTerm 没开时直接丢消息不会卡住 agent**。
2. 修改各 agent 的配置文件指向 `~/.chatterm/hook.sh`：

    | Agent | 配置文件 | 生效方式 |
    |---|---|---|
    | Claude Code | `~/.claude/settings.json` | 全局 —— 重启 Claude Code |
    | Codex | `~/.codex/hooks.json` + `config.toml` | 全局 —— 重启 Codex |
    | Kiro CLI | `~/.kiro/agents/chatterm.json` | **按 agent** —— 见 [激活 Kiro CLI hooks](#激活-kiro-cli-hooks) |

安装器是幂等的：重复跑会刷新过期的 ChatTerm 条目，不动其他 hook。

---

### 方式一 —— 一键安装（macOS 和 Debian/Ubuntu）

```bash
# 1. 安装 App（自动识别 macOS 还是 Linux）
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash

# 2. 配 agent hooks
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash
```

**macOS** 下拉 universal DMG（arm64 + x86_64），把 `ChatTerm.app` 拷到 `/Applications`。用 `curl` 下载避开了浏览器打的 `com.apple.quarantine` 标签，未签名的 app 不会被 Gatekeeper 拦截。

**Debian / Ubuntu** 下拉匹配架构的 `.deb`，跑 `sudo dpkg -i` + `sudo apt-get install -f`（会弹 sudo 密码）。

### 方式二 —— 手动下载

到 [Releases](https://github.com/chatterm/chatterm/releases) 选对应资产。

**macOS DMG** —— ChatTerm 未代码签名，浏览器下载的 DMG 双击会报「文件已损坏」。先剥 quarantine：

```bash
xattr -cr ~/Downloads/ChatTerm_*.dmg
# 挂载 DMG，把 ChatTerm 拖到 /Applications，然后：
bash /Applications/ChatTerm.app/Contents/Resources/setup-hooks.sh
```

**Debian / Ubuntu `.deb`**：

```bash
sudo dpkg -i ./chatterm_*.deb
sudo apt-get install -f
bash /usr/lib/chatterm/resources/setup-hooks.sh
```

**其他 Linux（`.AppImage`）**：

```bash
chmod +x ./ChatTerm_*.AppImage
./ChatTerm_*.AppImage
# AppImage 不自带 setup-hooks 资源，用远程方式：
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash
```

### 方式三 —— 从源码构建

```bash
git clone https://github.com/chatterm/chatterm.git && cd chatterm
npm install

# macOS
npm run tauri build && bash install.sh

# Debian / Ubuntu
npm run tauri -- build --bundles deb,appimage && bash scripts/install-linux.sh

# 配 hook（两个平台一样）
bash scripts/setup-hooks.sh
```

开发模式（Vite + Tauri dev 服务器）：

```bash
npm run tauri dev
```

**Ubuntu 开发依赖**（第一次跑 `tauri dev` / `tauri build` 前装一次）：

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file wget \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

### 激活 Kiro CLI hooks

Kiro CLI 从**当前激活的 agent 配置**读 hooks，不是全局。`setup-hooks.sh` 跑完后要切到 `chatterm` agent：

```bash
# 用 chatterm agent 启动新会话：
kiro-cli chat --agent chatterm

# 已有会话中切换：
/agent swap chatterm
```

想让 `chatterm` 成为默认 agent，改 `~/.kiro/settings.json`，或者设个 shell 别名：`alias kiro-cli='kiro-cli chat --agent chatterm'`。

### 推荐安装 Nerd Font

如果你的 shell prompt 用了 Powerline / Devicon 图标（starship、powerlevel10k、oh-my-zsh 主题等），装一个 Nerd Font 才能正常显示图标，否则会是空方框。ChatTerm 的 xterm fontFamily 已经优先匹配常见 Nerd Font 家族，装了就自动生效。

**Linux（Debian/Ubuntu）**

```bash
mkdir -p ~/.local/share/fonts
cd /tmp
wget https://github.com/ryanoasis/nerd-fonts/releases/download/v3.2.1/JetBrainsMono.zip
unzip -o JetBrainsMono.zip -d ~/.local/share/fonts/JetBrainsMonoNerd
fc-cache -fv ~/.local/share/fonts
```

**macOS（Homebrew）**

```bash
brew install --cask font-jetbrains-mono-nerd-font
```

装完重启 ChatTerm。

## 快捷键

| 按键 | 操作 |
|------|------|
| ⌘K (macOS) / Ctrl+Shift+K (Linux) | 搜索会话 |
| ⌘N (macOS) / Ctrl+Shift+N (Linux) | 新建会话 |
| F11 | 切换全屏 |
| Esc | 关闭弹窗 |

## 项目结构

```
src/                        # 前端 (React + TypeScript)
├── App.tsx                 # 主应用，会话状态，PTY 集成
├── XtermPane.tsx           # xterm.js 终端渲染
├── Sidebar.tsx             # 会话列表与状态指示器
├── CmdK.tsx                # ⌘K 搜索弹窗
├── themes.ts               # 主题系统
├── types.ts                # 共享类型
└── Icons.tsx               # SVG 图标

src-tauri/src/              # 后端 (Rust)
├── lib.rs                  # Tauri 命令，FIFO IPC 监听
├── pty.rs                  # PTY 管理器，Agent 检测，vscreen
├── vscreen.rs              # 虚拟屏幕，用于状态检测
├── agent_config.rs         # 配置驱动的 Agent 匹配 (agents.json)
├── theme.rs                # 主题解析器；macOS 可导入 Terminal 主题
├── session.rs              # 会话元数据持久化
└── main.rs                 # 入口

scripts/                    # 安装 + hook 脚本
├── install-remote.sh       # macOS release 安装器
├── install-linux.sh        # Linux 本地 bundle 安装器
└── setup-hooks.sh          # Agent hook 安装器（写入 ~/.chatterm/hook.sh）

design/                     # 设计资源
├── logo/                   # Logo 导出（v3、v4，6 个变体）
└── src/                    # UI 原型 (JSX)
```

## 当前状态

- [x] IM 风格会话列表，带头像和状态
- [x] PTY 终端 + xterm.js
- [x] Agent 自动识别（Claude、Kiro、Codex）
- [x] Hook 驱动的 FIFO IPC 预览
- [x] 主题系统 + macOS Terminal 导入（macOS）
- [x] 会话持久化 + Agent 恢复
- [x] ⌘K 搜索、置顶、重命名、关闭

## 目标用户

- AI CLI 重度用户（Claude Code / Kiro / Codex）
- 嵌入式 / IoT 工程师
- 远程 Linux / SSH 开发者
- DevOps / SRE / 平台工程师

## 许可证

[MIT](LICENSE)
