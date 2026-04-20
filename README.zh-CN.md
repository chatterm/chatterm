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
- **主题系统** — 导入 macOS Terminal 主题，内置 ChatTerm / VS Code Dark / Dark+
- **会话持久化** — 重启后恢复会话列表，Agent 会话支持 `--resume` 恢复
- **⌘K 搜索** — 按名称、工作目录或输出快速搜索会话
- **Shell 预览** — 显示 Shell 会话的最后命令和工作目录

## 技术栈

- **前端**: React 19 + TypeScript + Vite 7 + xterm.js
- **后端**: Rust + Tauri 2 + portable-pty
- **IPC**: Named Pipe (FIFO) 用于 hook → 应用通信
- **主题**: 可配置，支持导入 macOS Terminal `.terminal` 配置

## 快速开始

```bash
npm install
npm run tauri dev
```

## 构建安装

```bash
npm run tauri build
bash install.sh
```

## 配置 Agent Hooks

```bash
bash scripts/setup-hooks.sh
```

为 Claude Code（`~/.claude/settings.json`）、Kiro CLI（`~/.kiro/agents/chatterm.json`）和 Codex（`~/.codex/hooks.json`）配置 hooks，通过 FIFO 管道发送通知。

## 快捷键

| 按键 | 操作 |
|------|------|
| ⌘K | 搜索会话 |
| ⌘N | 新建会话 |
| Esc | 关闭弹窗 |

## 项目结构

```
src/                        # 前端 (React + TypeScript)
├── App.tsx                 # 主应用，会话状态，PTY 集成
├── XtermPane.tsx           # xterm.js 终端渲染
├── Sidebar.tsx             # 会话列表与状态指示器
├── CmdK.tsx                # ⌘K 搜索弹窗
├── themes.ts               # 主题系统（导入 macOS Terminal 主题）
├── types.ts                # 共享类型
└── Icons.tsx               # SVG 图标

src-tauri/src/              # 后端 (Rust)
├── lib.rs                  # Tauri 命令，FIFO IPC 监听
├── pty.rs                  # PTY 管理器，Agent 检测，vscreen
├── vscreen.rs              # 虚拟屏幕，用于状态检测
├── agent_config.rs         # 配置驱动的 Agent 匹配 (agents.json)
├── theme.rs                # macOS Terminal 主题解析器
├── session.rs              # 会话元数据持久化
└── main.rs                 # 入口

scripts/                    # Hook 脚本
├── chatterm-hook.sh        # Hook → FIFO 桥接（所有 Agent）
└── setup-hooks.sh          # 一键安装 hooks

design/                     # 设计资源
├── logo/                   # Logo 导出（v3、v4，6 个变体）
└── src/                    # UI 原型 (JSX)
```

## 当前状态

- [x] IM 风格会话列表，带头像和状态
- [x] PTY 终端 + xterm.js
- [x] Agent 自动识别（Claude、Kiro、Codex）
- [x] Hook 驱动的 FIFO IPC 预览
- [x] 主题系统 + macOS Terminal 导入
- [x] 会话持久化 + Agent 恢复
- [x] ⌘K 搜索、置顶、重命名、关闭

## 目标用户

- AI CLI 重度用户（Claude Code / Kiro / Codex）
- 嵌入式 / IoT 工程师
- 远程 Linux / SSH 开发者
- DevOps / SRE / 平台工程师

## 许可证

MIT
