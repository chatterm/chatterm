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

### 一键安装（macOS 和 Debian/Ubuntu）

```bash
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/install-remote.sh | bash
```

脚本自动检测系统：

- **macOS**：下载 universal DMG（arm64 + x86_64），把 `ChatTerm.app` 复制到 `/Applications`。`curl` 不会像浏览器那样打 `com.apple.quarantine` 标签，所以未签名的 app 不会被 Gatekeeper 拦截。
- **Debian / Ubuntu**：下载匹配架构的 `.deb`，用 `sudo dpkg -i` + `sudo apt-get install -f` 安装（会弹 sudo 密码提示）。

### 手动下载

到 [Releases](https://github.com/chatterm/chatterm/releases) 选资产：

- **macOS DMG** —— 因为 ChatTerm 还没做代码签名，浏览器下载的 DMG 双击可能报「文件已损坏」。先剥掉 quarantine 标签：
  ```bash
  xattr -cr ~/Downloads/ChatTerm_*.dmg
  ```
  然后挂载并把 ChatTerm 拖到 `/Applications`。
- **Ubuntu `.deb`**：
  ```bash
  sudo dpkg -i ./chatterm_*.deb
  sudo apt-get install -f
  ```
- **其他 Linux（`.AppImage`）**：`chmod +x` 后直接运行。

## 开发

```bash
npm install
npm run tauri dev
```

### Ubuntu 开发依赖

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file wget \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

## 从源码构建

```bash
npm run tauri build
bash install.sh
```

Ubuntu/Linux：

```bash
npm run tauri -- build --bundles deb,appimage
bash scripts/install-linux.sh
```

## 配置 Agent Hooks

脚本会把 hook 写到 `~/.chatterm/hook.sh`，并修改各 agent 的配置指向它。三种入口，选一个：

```bash
# 跨平台（macOS 和 Linux 都可用）
curl -fsSL https://raw.githubusercontent.com/chatterm/chatterm/main/scripts/setup-hooks.sh | bash

# 通过 DMG / curl 在 macOS 安装后
bash /Applications/ChatTerm.app/Contents/Resources/setup-hooks.sh

# 通过 .deb 在 Debian/Ubuntu 安装后
bash /usr/lib/chatterm/resources/setup-hooks.sh

# 从仓库直接跑
bash scripts/setup-hooks.sh
```

这些入口效果一致：hook 统一落在 `~/.chatterm/hook.sh`，下列配置都引用它：

| Agent | 配置文件 | 生效方式 |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | **全局生效** —— 重启 Claude Code |
| Codex | `~/.codex/hooks.json` + `config.toml` | **全局生效** —— 重启 Codex |
| Kiro CLI | `~/.kiro/agents/chatterm.json` | **按 agent 生效** —— 见下文 |

### 激活 Kiro CLI 的 hooks

Kiro CLI 从**当前激活的 agent 配置**读 hooks，不是全局的。跑完 `setup-hooks.sh` 后，需要切到 `chatterm` agent：

```bash
# 用 chatterm agent 启动新会话：
kiro-cli chat --agent chatterm

# 已有会话中切换：
/agent swap chatterm
```

想让 `chatterm` 成为 Kiro 的默认 agent，可以改 `~/.kiro/settings.json`，或者设个 shell 别名：`alias kiro-cli='kiro-cli chat --agent chatterm'`。

## 快捷键

| 按键 | 操作 |
|------|------|
| ⌘K / Ctrl+K | 搜索会话 |
| ⌘N / Ctrl+N | 新建会话 |
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

MIT
