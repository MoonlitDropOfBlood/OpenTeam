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
- **多 Provider**：Anthropic Claude + DeepSeek V4 + OpenAI 兼容 + Ollama + GROQ + OpenRouter + xAI
- **OpenCode 兼容配置**：完整的 provider + model 配置体系，支持 `base_url`、`timeout`、`apiKey`、自定义 headers
- **Agent 级配置**：每个 Agent 独立配置 primary + fallback 模型
- **推理模式**：支持 Anthropic thinking 预算 + DeepSeek 推理模式 + OpenAI reasoning_effort
- **智能重试**：指数退避 + 20% 抖动 + Retry-After 头解析
- **结构化错误**：LlmAuth / LlmRateLimit / LlmApi 分类错误处理
- **限流保护**：滑动窗口 RPM 限流器
- **超额重试保护**：401/403/400 等非可重试错误不重试

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
    model: anthropic/claude-sonnet-4-20250514
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
    model: deepseek/deepseek-v4-pro
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }
  fallback:
    model: deepseek/deepseek-v4-flash
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
    model: anthropic/claude-sonnet-4-20250514   # 必填: {provider}/{model_name}
    api_key_env: ANTHROPIC_API_KEY              # 可选: API Key 环境变量名
    max_tokens: 8192                            # 必填: 最大输出 token
    timeout_secs: 120                           # 可选: API 超时（秒）
    rate_limit: { rpm: 50, tpm: 100000 }        # 可选: 速率限制
    temperature: 0.7                            # 可选: 采样温度
    top_p: 0.9                                  # 可选: 核采样
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `model` | String | 格式 `{provider}/{model_name}`。Provider 从 `/` 前自动提取，如 `anthropic/claude-sonnet-4-20250514` |
| `api_key_env` | String | API Key 对应的环境变量名。不设置则使用默认名（`ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` / `OPENAI_API_KEY`） |
| `max_tokens` | u32 | 最大输出 token 数 |
| `timeout_secs` | u64 | API 调用超时（默认 120s） |
| `rate_limit` | Object | `{ rpm: number, tpm: number }` 速率限制 |
| `temperature` | f64 | 采样温度 (0-2) |
| `top_p` | f64 | 核采样参数 (0-1) |
| `top_k` | u32 | Top-K 采样 |
| `stop` | String[] | 停止序列 |
| `presence_penalty` | f64 | 存在惩罚 (OpenAI 兼容) |
| `frequency_penalty` | f64 | 频率惩罚 (OpenAI 兼容) |
| `reasoning_effort` | String | Anthropic 思考预算: `low` / `medium` / `high` |
| `thinking` | bool | DeepSeek 推理模式。启用时强制 temperature=1 |
| `max_retries` | u32 | API 失败最大重试次数 |
| `skip_verify_ssl` | bool | 跳过 SSL 证书验证 |
| `base_url` | String | 自定义 API 端点，覆盖 provider 默认地址 |

#### Provider 配置块（`llm_config.yaml`）

`llm_config.yaml` 定义 Provider 级别配置和全局模型池。每个 Provider 有单独的 options（baseURL、timeout、cache key 等）：

```yaml
provider:
  anthropic:
    name: "Anthropic"
    env: ["ANTHROPIC_API_KEY"]
    options:
      baseURL: https://api.anthropic.com/v1
      timeout: 300000
      setCacheKey: true
    models:
      claude-sonnet-4-20250514:
        name: "Claude Sonnet 4"
        limit: { context: 200000, output: 50000 }

  deepseek:
    name: "DeepSeek"
    env: ["DEEPSEEK_API_KEY"]
    options:
      baseURL: https://api.deepseek.com/v1
      timeout: 300000
    models:
      deepseek-v4-pro:
        name: "DeepSeek V4 Pro"
        limit: { context: 64000, output: 8192 }

  openai:
    name: "OpenAI"
    env: ["OPENAI_API_KEY"]
    options:
      baseURL: https://api.openai.com/v1
      timeout: 300000
    models:
      gpt-4o:
        name: "GPT 4o"
        limit: { context: 128000, output: 4096 }

  ollama:
    name: "Ollama (local)"
    options:
      baseURL: http://localhost:11434/api
      timeout: 60000

  groq:
    name: "GROQ"
    env: ["GROQ_API_KEY"]
    options:
      baseURL: https://api.groq.com/openai/v1
      timeout: 300000

  openrouter:
    name: "OpenRouter"
    env: ["OPENROUTER_API_KEY"]
    options:
      baseURL: https://openrouter.ai/api/v1
      timeout: 300000

  xai:
    name: "xAI"
    env: ["XAI_API_KEY"]
    options:
      baseURL: https://api.x.ai/v1
      timeout: 300000
```

| Provider 选项 | 类型 | 说明 |
|---------------|------|------|
| `options.baseURL` | String | API 端点地址 |
| `options.timeout` | Number | 请求超时（毫秒，默认 300000） |
| `options.apiKey` | String | API Key（支持 `{env:VAR}` 引用） |
| `options.headers` | Object | 自定义 HTTP 请求头 |
| `options.setCacheKey` | Bool | 启用 Prompt Cache（Anthropic/DeepSeek） |
| `options.chunkTimeout` | Number | SSE 流式 chunk 超时（毫秒） |

#### 全局模型池（兼容旧格式）

保留旧 `models:` 格式以兼容已有配置，但新配置请使用 `provider:` 块：

```yaml
models:
  claude-sonnet-4:
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }

**支持的 Provider：**

| Provider | API 端点 | 认证 | 类型 |
|----------|---------|------|------|
| `anthropic` | `https://api.anthropic.com/v1/messages` | `ANTHROPIC_API_KEY` | 原生 |
| `deepseek` | `https://api.deepseek.com/v1/chat/completions` | `DEEPSEEK_API_KEY` | OpenAI 兼容 |
| `openai` | `https://api.openai.com/v1/chat/completions` | `OPENAI_API_KEY` | OpenAI 兼容 |
| `groq` | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` | OpenAI 兼容 |
| `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` | OpenAI 兼容 |
| `xai` | `https://api.x.ai/v1` | `XAI_API_KEY` | OpenAI 兼容 |
| `ollama` | `http://localhost:11434/api/chat` | 无需认证 | Ollama 原生 |

`base_url` 可覆盖任意 provider 的默认端点，实现任意 OpenAI 兼容 API 接入。OpenRouter 会自动添加 `HTTP-Referer` 和 `X-Title` 头。

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

**内建 Skill（自动释放）：** 系统内置 `feishu-doc` skill（飞书文档管理），首次启动时自动释放到全局技能目录。无需手动创建。

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

# 运行测试（81 tests）
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
    model: deepseek/deepseek-v4-flash
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

系统会自动释放内建 Skill（如 `feishu-doc`）到 `~/.config/OpenTeam/skills/`，无需手动创建。如需添加自定义 Skill：

```bash
mkdir -p ~/.config/OpenTeam/skills/my-custom-skill
cat > ~/.config/OpenTeam/skills/my-custom-skill/SKILL.md << 'EOF'
---
name: my-custom-skill
description: Your custom skill
---

# My Custom Skill

Describe what this skill does here.
EOF
```

### 第七步：配置飞书（**必选**）

飞书集成现在为强制要求。启动前必须完成以下配置：

```bash
# 1. 登录飞书 CLI
lark-cli login

# 2. 验证登录
lark-cli auth check

# 3. 设置飞书群 ID（用于发送消息）
export FEISHU_CHAT_ID=oc_xxxxxxxxxxxx
```

缺少任一配置，系统将在启动时报错并退出。

配置状态可在 TUI F5 页面查看。

## 项目结构

```
├── Cargo.toml                     # Workspace 定义
├── llm_config.yaml                # 全局 LLM 模型池 + Provider 配置
├── agents/                        # Agent YAML 配置
│   └── pm.yaml                    # 示例 Agent
├── crates/core/                   # Rust Core 库
│   └── src/
│       ├── config/                # 配置解析（含 provider 配置）
│       ├── registry/              # Agent 注册中心
│       ├── llm/                   # LLM Gateway + Provider 分辨率 + 内建模型定义
│       │   ├── gateway.rs         # LLM HTTP 请求/响应（3 个 provider 实现）
│       │   ├── models.rs          # 内建模型定义（22 个模型，成本/容量/能力）
│       │   ├── provider.rs        # ProviderResolver 三层分辨率链
│       │   └── rate_limiter.rs    # 滑动窗口 RPM 限流器
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
| LLM | Anthropic Claude / DeepSeek V4 / OpenAI / GROQ / OpenRouter / xAI / Ollama |
| 存储 | SQLite (sqlx) |
| 向量化 | ONNX Runtime / hash-based fallback |
| 飞书 | lark-cli |
| MCP | stdio / HTTP 传输，标准 JSON-RPC |
| 插件 | Node.js + JSON-RPC over stdio |
| 文件监控 | notify (热重载) |

## 许可证

[Apache License 2.0](LICENSE)