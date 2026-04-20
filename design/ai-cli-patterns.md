# AI CLI Agent Screen Patterns (from real PTY captures)

## Kiro CLI

### States
| Screen content | State |
|---|---|
| `⠀ Thinking... (esc to cancel)` | **thinking** (braille spinner `⠀⠁⠂⠃...` animates) |
| `Kiro is working · type to queue a message` | **working** (processing, accepting queued input) |
| `Kiro · auto · ◔ N%` | **idle** (status bar visible, waiting for input) |
| `● Initializing...` | **starting** |

### Line types
| Pattern | Type | Example |
|---|---|---|
| `  {text}` (2-space indent, in conversation area) | User input OR LLM reply | `  你好` |
| `⠀ Thinking... (esc to cancel)` | Thinking indicator | — |
| `▸ Credits: N.NN • Time: Ns` | Cost info (chrome) | `▸ Credits: 0.14 • Time: 5s` |
| `● N MCP failure — see /mcp` | Error (chrome) | — |
| `Kiro · auto · ◔ N%` + path | Status bar (chrome) | — |
| `ask a question or describe a task ↵` | Input placeholder (chrome) | — |
| `/copy to clipboard` | Hint (chrome) | — |
| `Kiro is working · type to queue a message` | Working indicator (chrome) | — |
| `Welcome to the new Kiro CLI UX!...` | Banner (chrome) | — |

### Preview extraction
- Last `  {text}` line that is NOT the user's input (need to track which was sent)
- Skip: Thinking, Credits, status bar, MCP errors, banner, placeholder

---

## OpenAI Codex

### States
| Screen content | State |
|---|---|
| `• Working (Ns • esc to interrupt)` | **thinking** |
| `• Starting MCP servers...` | **starting** |
| `› ` prompt visible, no Working | **idle** |

### Line types
| Pattern | Type | Example |
|---|---|---|
| `› {text}` | **User input** | `› 你好` |
| `• {text}` (NOT Working/Starting/Explored) | **LLM reply** | `• 你好。有什么需要我处理的？` |
| `• Working (Ns • esc to interrupt)` | Thinking (chrome) | — |
| `• Starting MCP servers...` | Starting (chrome) | — |
| `• Explored` | Tool use header | — |
| `  └ {action} {command}` | Tool detail | `  └ List ls -la` |
| `gpt-X.X xhigh · ~/path` | Status bar (chrome) | — |
| `>_ OpenAI Codex (vX.X.X)` | Banner (chrome) | — |
| `model: gpt-X.X xhigh /model to change` | Model info (chrome) | — |
| `directory: ~/path` | Directory info (chrome) | — |
| `Tip: ...` | Tip (chrome) | — |
| `⚠ MCP client...` | Error (chrome) | — |
| `› Implement {feature}` | Prompt suggestion (chrome) | — |

### Preview extraction
- Last `• {text}` line where text is NOT "Working"/"Starting"/"Explored"
- Multi-line replies: lines after `•` without prefix are continuation
- Skip: Working, Starting, Explored, status bar, banner, tips, errors

---

## Claude Code

### States (from OSC title)
| OSC title prefix | State |
|---|---|
| `✳ Claude Code` | **idle** |
| `⠂ Claude Code` (braille spinner) | **thinking** |
| `✢`/`✶`/`✻`/`✽` + verb | **thinking** (e.g., `✢ Proofing…`, `✳ Moonwalking…`) |

### Line types
| Pattern | Type | Example |
|---|---|---|
| `❯ {text}` | **User input** | `❯ 你好` |
| `● {text}` | **LLM reply** | `● 你好！有什么可以帮你的吗？` |
| `✢ {Verb}…` / `✳ {Verb}…` etc. | Thinking spinner (chrome) | `✢ Precipitating…` |
| `Context N% │ Usage N%...` | Status bar (chrome) | — |
| `[Opus 4.7 (1M context) │ Max]` | Model info (chrome) | — |
| `N MCPs` | MCP count (chrome) | — |
| `◉ xhigh · /effort` | Effort indicator (chrome) | — |
| `Claude Code vX.X.X` | Version banner (chrome) | — |
| `Welcome back {name}!` | Welcome (chrome) | — |
| `Tips for getting started` | Tips (chrome) | — |

### Preview extraction
- Last `● {text}` line (strip the `●` prefix)
- Skip: ❯ (user input), spinners, status bar, model info, banner, tips
