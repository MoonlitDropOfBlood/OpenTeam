# OpenTeam

> **在飞书群里开一家"一人公司"。**
>
> 自定义 AI Agent（数字员工），在飞书群中像真实团队一样自动协作。
> 用户（老板）在群里发需求，Agent 们自动拆解任务、互相 @ 协作。

[English](./README.en.md) | [中文](./README.md)

---

## 功能特性

### Agent 系统
- **自定义 Agent**：通过 YAML 定义角色、性格、LLM 配置、触发词
- **助理 Agent**：主 Agent，忙时主动推进，闲时静默归档，紧急关键词立即唤醒
- **双通道通信**：用户可 @助理（日常协调）或直接 @任意 Agent（最高优先级）
- **忙闲感知**：周一至周五 9:00-18:00 忙时模式，其余时间闲时模式，紧急关键词覆盖一切

### 记忆系统
- **三层记忆**：Working Memory（LLM 上下文）→ Short-term（结构化摘要）→ Long-term（持久知识）
- **语义检索**：余弦相似度向量搜索
- **自然遗忘**：艾宾浩斯衰减模型，重要性 8-10 近永久保留
- **自动压缩**：短对话规则提取，长对话 LLM 压缩
- **SQLite 存储**：本地持久化，零外部依赖

### LLM 集成
- **多 Provider**：Anthropic Claude + DeepSeek V4 + Ollama 本地模型
- **Agent 级配置**：每个 Agent 独立配置 primary + fallback 模型
- **限流保护**：滑动窗口 RPM 限流器
- **超时控制**：可配置 API 超时

### 飞书集成
- **消息收发**：发送/回复/话题线程回复
- **@mention**：`<at user_id="xxx">名称</at>` 格式
- **WebSocket 事件订阅**：实时接收飞书消息事件
- **发送队列**：5 QPS 限流，优先级排序
- **自动重试**：失败自动降级

### 插件系统
- **Node.js 宿主**：JSON-RPC over stdio 通信
- **Hook 点**：system:startup/shutdown、message:received、agent:after_create 等
- **Plugin SDK**：`createPlugin()` 快速开发插件

### TUI 终端界面
| 快捷键 | 页面 | 内容 |
|--------|------|------|
| F1 | Home | Agent 卡片 + 消息流 |
| F2 | Agents | Agent 管理列表 |
| F3 | Tasks | 任务看板 |
| F4 | Logs | 日志查看器 |
| F5 | Feishu | 飞书连接状态 |
| F6 | Memory | 记忆浏览器 |
| `r` | — | 手动刷新数据 |
| `a` | — | 切换自动刷新 (5s) |
| `q` / `Esc` | — | 退出 |

---

## 配置参考

系统有四种配置：Agent 定义、LLM 模型池、MCP 工具、Skill 技能。全部支持文件热重载。

### Agent YAML 配置

Agent 通过 YAML 文件定义，放在 `agents/` 目录下自动加载。

**最小配置：**
```yaml
name: "CodeCat"
role: "You are a senior backend engineer."
llm:
  primary:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
```

**完整配置：**
```yaml
name: "CodeCat"
role: "You are a senior backend engineer, skilled in Rust and system architecture."
personality: "Rigorous, organized, code-quality focused"
llm:
  primary:
    provider: deepseek
    model: deepseek-v4-pro
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }
  fallback:
    provider: deepseek
    model: deepseek-v4-flash
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 4096
    timeout_secs: 60
triggers:
  - pattern: "开发|实现|代码|架构"
    auto_respond: true
  - pattern: "@Dev|@CodeCat"
    auto_respond: true
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | String | ✅ | Agent 名称（用于 @mention） |
| `role` | String | ✅ | System Prompt，定义角色行为 |
| `personality` | String | — | 性格描述 |
| `llm.primary` | Object | ✅ | 主 LLM 配置（见下方 ModelConfig） |
| `llm.fallback` | Object | — | 降级 LLM 配置（primary 失败时自动切换） |
| `triggers[].pattern` | String | — | 触发关键词（正则表达式） |
| `triggers[].auto_respond` | Bool | — | 是否自动响应（默认 `true`） |

Agent 的 `skills` 和 `mcps` 字段已移除 —— 现在通过文件系统自动发现（见下方）。

---

### LLM 配置

#### ModelConfig（Agent YAML 内嵌）

每个 Agent 在 `llm.primary` 中定义自己的模型：

```yaml
llm:
  primary:
    provider: anthropic                    # 必填: anthropic | ollama | deepseek
    model: claude-sonnet-4-20250514        # 必填: 模型 ID
    api_key_env: ANTHROPIC_API_KEY         # 可选: API Key 环境变量名
    max_tokens: 8192                       # 必填: 最大输出 token
    timeout_secs: 120                      # 可选: API 超时（秒）
    rate_limit: { rpm: 50, tpm: 100000 }   # 可选: 速率限制
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `provider` | String | 模型提供商。`anthropic` / `ollama` / `deepseek` |
| `model` | String | 模型 ID。如 `claude-sonnet-4-20250514`、`deepseek-v4-pro`、`qwen2.5:3b` |
| `api_key_env` | String | API Key 对应的环境变量名。不设置则使用默认名（`ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY`） |
| `max_tokens` | u32 | 最大输出 token 数 |
| `timeout_secs` | u64 | API 调用超时（默认 120s） |
| `rate_limit` | Object | `{ rpm: number, tpm: number }` 速率限制 |

#### 全局模型池（可选）

`llm_config.yaml` 定义共享模型池，Agent 可以通过此池引用模型（Agent 内嵌配置优先级更高）：

```yaml
models:
  claude-sonnet-4:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }

  deepseek-v4-pro:
    provider: deepseek
    model: deepseek-v4-pro
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 200000 }

  deepseek-v4-flash:
    provider: deepseek
    model: deepseek-v4-flash
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 100, tpm: 500000 }

  ollama-qwen:
    provider: ollama
    model: qwen2.5:3b
    max_tokens: 4096
    timeout_secs: 60
```

**支持的 Provider：**

| Provider | API 端点 | 认证 |
|----------|---------|------|
| `anthropic` | `https://api.anthropic.com/v1/messages` | `ANTHROPIC_API_KEY` |
| `deepseek` | `https://api.deepseek.com/v1/chat/completions` | `DEEPSEEK_API_KEY` |
| `ollama` | `http://localhost:11434/api/chat` | 无需认证 |

---

### MCP 配置（工具调用）

MCP 服务器配置放在 `~/.config/OpenTeam/mcp.json`。支持本地子进程和远程 HTTP 两种传输方式。

**标准格式：**

```json
{
  "mcpServers": {
    "github": {
      "command": "node",
      "args": ["path/to/github-mcp-server"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      },
      "enabled": true
    },
    "remote-api": {
      "url": "https://api.example.com/mcp",
      "headers": {
        "Authorization": "Bearer ${API_TOKEN}"
      },
      "enabled": true
    }
  }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `command` | String | 互斥 | 本地服务器可执行文件路径（stdio 传输） |
| `url` | String | 互斥 | 远程服务器 URL（HTTP 传输） |
| `args` | String[] | — | 命令行参数 |
| `env` | Object | — | 环境变量（支持 `${VAR_NAME}` 插值） |
| `headers` | Object | — | HTTP 请求头（仅远程模式） |
| `enabled` | Bool | — | 是否启用（默认 `true`，设为 `false` 跳过） |

工具定义不在配置文件中 —— 启动时自动向服务器发送 `tools/list` JSON-RPC 请求动态发现。

**文件热重载：** `mcp.json` 文件变更自动触发重新探测，无需重启。

---

### Skill 配置（技能注入）

Skill 定义 Agent 可以使用的专项能力。自动从三个目录发现和合并：

| 发现源 | 路径 | 作用域 |
|--------|------|--------|
| 全局 | `~/.config/OpenTeam/skills/<name>/SKILL.md` | 所有 Agent 共享 |
| Agent 专属 | `agents/<agent-name>/skills/<name>/SKILL.md` | 仅该 Agent |
| 助理 | `~/.config/OpenTeam/assistant/skills/<name>/SKILL.md` | 助理 Agent |

**SKILL.md 格式：**
```markdown
---
name: feishu-doc
description: Create and manage Feishu documents
---

# Feishu Doc Skill

## Instructions
You can create and edit Feishu documents. When asked to write documentation:

1. Create a new Feishu doc using `lark-cli doc +create --title <title>`
2. Write content using Feishu markdown format
3. Share the doc link in your response
```

Skill 内容自动注入 Agent 的 LLM System Prompt，作为"可用技能"区块。

**文件热重载：** `SKILL.md` 文件变更自动生效。

---

### 内建工具

无需配置，所有 Agent 自动可用：

| 工具 | 说明 |
|------|------|
| `read_file` | 读取文件（UTF-8，≤100KB） |
| `write_file` | 写入文件（自动创建目录） |
| `glob_files` | 通配符搜索文件（≤100 结果） |
| `grep_search` | 正则搜索文件内容 |
| `list_directory` | 列出目录 |
| `bash_exec` | 执行 Shell 命令（10s 超时） |
| `web_fetch` | 获取 URL 内容 |
| `send_feishu_message` | 发送飞书消息（支持话题回复） |

---

## 快速开始

### 前置条件

| 依赖 | 版本 | 用途 | 安装 |
|------|------|------|------|
| Rust | ≥ 1.75 | 核心引擎 | [rustup](https://rustup.rs/) |
| Node.js | ≥ 18 | 插件宿主 | [nodejs.org](https://nodejs.org/) |
| lark-cli | 最新 | 飞书集成 | [飞书 CLI 指南](https://open.feishu.cn/document/uYjL24iN/uMDMxEjLzAjMx4yM0ITM) |
| Ollama (可选) | ≥ 0.1 | 本地 LLM | [ollama.com](https://ollama.com/) |

### 第一步：克隆 & 构建

```bash
git clone <repo-url>
cd OpenTeam

# 构建全部（首次约 2-3 分钟）
cargo build

# 运行测试（57 tests）
cargo test
```

### 第二步：配置 API 密钥

根据你使用的 LLM 设置环境变量：

```bash
# Windows PowerShell
$env:ANTHROPIC_API_KEY = "sk-ant-xxx"
$env:DEEPSEEK_API_KEY = "sk-ds-xxx"

# macOS / Linux
export ANTHROPIC_API_KEY=sk-ant-xxx
export DEEPSEEK_API_KEY=sk-ds-xxx
```

如果使用 Ollama 本地模型（无需 API Key）：

```bash
ollama pull qwen2.5:3b
ollama serve
```

### 第三步：创建 Agent

`agents/pm.yaml` 已自带一个示例 Agent（产品经理"小红"）。你也可以创建自己的：

**`agents/dev.yaml`：**
```yaml
name: "CodeCat"
role: "You are a senior backend engineer."
llm:
  primary:
    provider: deepseek
    model: deepseek-v4-flash
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
triggers:
  - pattern: "@CodeCat|@Dev"
    auto_respond: true
```

### 第四步：运行

```bash
# 启动 TUI 终端界面
cargo run -p feishu-agent-tui
```

在 TUI 中按 **F1** 查看 Agent 列表，**F2** 查看详情，**F5** 查看飞书状态。

### 第五步（可选）：配置 MCP 工具

```bash
mkdir -p ~/.config/OpenTeam
cat > ~/.config/OpenTeam/mcp.json << 'EOF'
{
  "mcpServers": {}
}
EOF
```

### 第六步（可选）：配置 Skill

```bash
mkdir -p ~/.config/OpenTeam/skills/feishu-doc
cat > ~/.config/OpenTeam/skills/feishu-doc/SKILL.md << 'EOF'
---
name: feishu-doc
description: Create and manage Feishu documents
---

# Feishu Doc Skill

You can create Feishu documents using lark-cli.
EOF
```

### 第七步（可选）：配置飞书

```bash
# 登录飞书 CLI
lark-cli login

# 验证登录
lark-cli auth check

# 设置飞书群 ID（用于发送消息）
export FEISHU_CHAT_ID=oc_xxxxxxxxxxxx
```

配置状态可在 TUI F5 页面查看。

## 项目结构

```
├── Cargo.toml                     # Workspace 定义
├── llm_config.yaml                # 全局 LLM 模型池
├── agents/                        # Agent YAML 配置
│   └── pm.yaml                    # 示例 Agent
├── crates/core/                   # Rust Core 库
│   └── src/
│       ├── config/                # 配置解析
│       ├── registry/              # Agent 注册中心
│       ├── llm/                   # LLM Gateway
│       ├── feishu/                # 飞书 CLI Bridge
│       ├── memory/                # 三层记忆系统
│       ├── agent/                 # Agent 生命周期
│       ├── router/                # 消息路由
│       ├── assistant/             # 助理 Agent
│       ├── mcp/                   # MCP 工具执行引擎
│       ├── skill/                 # Skill 自动发现
│       ├── plugin/                # 插件管理器
│       └── error.rs               # 统一错误类型
├── crates/tui/                    # TUI 终端界面
├── plugins/                       # Node.js 插件宿主
└── docs/superpowers/              # 设计文档
```

## 测试

```bash
# 运行所有测试
cargo test --workspace

# 只运行单元测试
cargo test --lib

# 运行特定模块测试
cargo test memory::forgetting
cargo test assistant::time_policy

# 运行冒烟测试
cargo test --test smoke_test
```

## 技术栈

| 层 | 技术 |
|---|------|
| 核心引擎 | Rust (Tokio async) |
| TUI | Ratatui + Crossterm |
| LLM | Anthropic Claude / DeepSeek V4 / Ollama |
| 存储 | SQLite (sqlx) |
| 向量化 | ONNX Runtime / hash-based fallback |
| 飞书 | lark-cli |
| MCP | stdio / HTTP 传输，标准 JSON-RPC |
| 插件 | Node.js + JSON-RPC over stdio |
| 文件监控 | notify (热重载) |

## 许可证

[Apache License 2.0](LICENSE)