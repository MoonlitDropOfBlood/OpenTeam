# OpenTeam

> **Run a one-person company from your Feishu group chat.**
>
> Create custom AI Agents (digital employees) that collaborate automatically in Feishu groups like a real team.
> The user (boss) posts requirements in the group, and Agents break down tasks, @mention each other, and get work done.

[中文文档](./README.md)

---

## Features

### Agent System
- **Custom Agents**: Define roles, personality, LLM config, and trigger keywords via YAML
- **Assistant Agent**: Main agent — proactively pushes during busy hours, silently archives during idle, immediately responds to urgent keywords
- **Dual-channel Communication**: User can @assistant (daily coordination) or directly @any Agent (highest priority)
- **Busy/Idle Awareness**: Weekdays 9:00-18:00 = busy mode; other times = idle mode; urgent keywords override all

### Memory System
- **Three-tier Memory**: Working Memory (LLM context) → Short-term (structured summaries) → Long-term (persistent knowledge)
- **Semantic Search**: Cosine similarity vector search
- **Natural Forgetting**: Ebbinghaus decay model — importance 8-10 is near-permanent
- **Auto Compression**: Rule-based for short conversations, LLM-based for long ones
- **SQLite Storage**: Local persistence, zero external dependencies

### LLM Integration
- **Multiple Providers**: Anthropic Claude + DeepSeek V4 + Ollama local models
- **Per-Agent Config**: Each Agent independently configures primary + fallback models
- **Rate Limiting**: Sliding window RPM limiter
- **Timeout Control**: Configurable API timeout per model

### Feishu Integration
- **Messaging**: Send/reply/thread replies
- **@mention**: `<at user_id="xxx">Name</at>` format
- **WebSocket Events**: Real-time Feishu message event subscription
- **Send Queue**: 5 QPS rate limiting with priority ordering
- **Auto Retry**: Automatic fallback on failure

### Plugin System
- **Node.js Host**: JSON-RPC over stdio communication
- **Hook Points**: system:startup/shutdown, message:received, agent:after_create, etc.
- **Plugin SDK**: `createPlugin()` for rapid plugin development

### Built-in Tools
Available to all agents without configuration:

| Tool | Description |
|------|-------------|
| `read_file` | Read a file (UTF-8, ≤100KB) |
| `write_file` | Write to a file (auto-creates directories) |
| `glob_files` | Find files by glob pattern (≤100 results) |
| `grep_search` | Regex search in file contents |
| `list_directory` | List directory entries |
| `bash_exec` | Execute a shell command (10s timeout) |
| `web_fetch` | Fetch URL content |
| `send_feishu_message` | Send a Feishu message (supports thread replies) |

### TUI Terminal Interface
| Key | Page | Content |
|-----|------|---------|
| F1 | Home | Agent cards + message feed |
| F2 | Agents | Agent management list |
| F3 | Tasks | Task board |
| F4 | Logs | Log viewer |
| F5 | Feishu | Feishu connection status |
| F6 | Memory | Memory browser |
| `r` | — | Manual refresh |
| `a` | — | Toggle auto-refresh (5s interval) |
| `q` / `Esc` | — | Quit |

---

## Configuration Reference

The system has four configuration types: Agent definitions, LLM model pool, MCP tools, and Skills. All support hot-reload via file system watching.

### Agent YAML Config

Agent definitions go in the `agents/` directory and are auto-discovered on startup.

**Minimum config:**
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

**Full config:**
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
  - pattern: "develop|implement|code|architecture"
    auto_respond: true
  - pattern: "@Dev|@CodeCat"
    auto_respond: true
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | ✅ | Agent name (used for @mention) |
| `role` | String | ✅ | System prompt defining agent behavior |
| `personality` | String | — | Personality description |
| `llm.primary` | Object | ✅ | Primary LLM config |
| `llm.fallback` | Object | — | Fallback LLM config (auto-switch on primary failure) |
| `triggers[].pattern` | String | — | Trigger keywords (regex) |
| `triggers[].auto_respond` | Bool | — | Auto-respond (default `true`) |

The `skills` and `mcps` fields have been removed from Agent YAML — they are now auto-discovered from the filesystem (see below).

---

### LLM Configuration

#### ModelConfig (embedded in Agent YAML)

| Field | Type | Description |
|-------|------|-------------|
| `provider` | String | `anthropic` / `ollama` / `deepseek` |
| `model` | String | Model ID (e.g. `claude-sonnet-4-20250514`, `deepseek-v4-pro`) |
| `api_key_env` | String | Env var name for API key. Defaults: `ANTHROPIC_API_KEY` / `DEEPSEEK_API_KEY` |
| `max_tokens` | u32 | Maximum output tokens |
| `timeout_secs` | u64 | API timeout in seconds (default 120) |
| `rate_limit` | Object | `{ rpm: number, tpm: number }` |

#### Global Model Pool (optional)

`llm_config.yaml` defines a shared model pool:

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

**Supported Providers:**

| Provider | API Endpoint | Auth |
|----------|-------------|------|
| `anthropic` | `https://api.anthropic.com/v1/messages` | `ANTHROPIC_API_KEY` |
| `deepseek` | `https://api.deepseek.com/v1/chat/completions` | `DEEPSEEK_API_KEY` |
| `ollama` | `http://localhost:11434/api/chat` | None required |

---

### MCP Configuration (Tool Calling)

MCP server configs go in `~/.config/OpenTeam/mcp.json`. Supports both local subprocess (stdio) and remote HTTP transport.

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

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | String | Mutual | Local server executable (stdio transport) |
| `url` | String | Mutual | Remote server URL (HTTP transport) |
| `args` | String[] | — | Command-line arguments |
| `env` | Object | — | Environment variables (supports `${VAR_NAME}` interpolation) |
| `headers` | Object | — | HTTP headers (remote only) |
| `enabled` | Bool | — | Enable/disable (default `true`, set `false` to skip) |

Tool definitions are NOT in the config file — they are dynamically discovered at startup by sending `tools/list` JSON-RPC to each server.

**Hot-reload:** Changes to `mcp.json` automatically trigger re-discovery.

---

### Skill Configuration

Skills define specialized capabilities for agents. Auto-discovered from three sources:

| Source | Path | Scope |
|--------|------|-------|
| Global | `~/.config/OpenTeam/skills/<name>/SKILL.md` | All agents |
| Per-agent | `agents/<agent-name>/skills/<name>/SKILL.md` | Specific agent only |
| Assistant | `~/.config/OpenTeam/assistant/skills/<name>/SKILL.md` | Assistant agent |

**SKILL.md format:**
```markdown
---
name: feishu-doc
description: Create and manage Feishu documents
---

# Feishu Doc Skill

## Instructions
You can create and edit Feishu documents using `lark-cli`.
```

Skill content is automatically injected into the agent's LLM System Prompt under "Available Skills".

**Hot-reload:** Changes to `SKILL.md` take effect immediately.

---

## Quick Start

### Prerequisites

| Dependency | Version | Use | Install |
|------------|---------|-----|---------|
| Rust | ≥ 1.75 | Core engine | [rustup](https://rustup.rs/) |
| Node.js | ≥ 18 | Plugin host | [nodejs.org](https://nodejs.org/) |
| lark-cli | Latest | Feishu integration | [Feishu CLI Guide](https://open.feishu.cn/document/uYjL24iN/uMDMxEjLzAjMx4yM0ITM) |
| Ollama (optional) | ≥ 0.1 | Local LLM | [ollama.com](https://ollama.com/) |

### Step 1: Clone & Build

```bash
git clone <repo-url>
cd OpenTeam

cargo build
cargo test
```

### Step 2: Set API Keys

```bash
export ANTHROPIC_API_KEY=sk-ant-xxx
export DEEPSEEK_API_KEY=sk-ds-xxx
```

For local Ollama (no API key required):
```bash
ollama pull qwen2.5:3b
ollama serve
```

### Step 3: Create an Agent

`agents/pm.yaml` comes with a sample Agent. Create your own:

**`agents/dev.yaml`:**
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

### Step 4: Run

```bash
cargo run -p feishu-agent-tui
```

Press **F1** to view agents, **F2** for details, **F5** for Feishu status.

### Step 5 (Optional): Configure MCP

```bash
mkdir -p ~/.config/OpenTeam
cat > ~/.config/OpenTeam/mcp.json << 'EOF'
{
  "mcpServers": {}
}
EOF
```

### Step 6 (Optional): Configure Skills

```bash
mkdir -p ~/.config/OpenTeam/skills/feishu-doc
cat > ~/.config/OpenTeam/skills/feishu-doc/SKILL.md << 'EOF'
---
name: feishu-doc
description: Create and manage Feishu documents
---
# Feishu Doc Skill
Create Feishu documents using lark-cli.
EOF
```

### Step 7 (Optional): Configure Feishu

```bash
lark-cli login
lark-cli auth check
export FEISHU_CHAT_ID=oc_xxxxxxxxxxxx
```

## Project Structure

```
├── Cargo.toml                     # Workspace definition
├── llm_config.yaml                # Global LLM model pool
├── agents/                        # Agent YAML configs
│   └── pm.yaml                    # Sample agent
├── crates/core/                   # Rust Core library
│   └── src/
│       ├── config/                # Config parsing
│       ├── registry/              # Agent registry
│       ├── llm/                   # LLM Gateway
│       ├── feishu/                # Feishu CLI Bridge
│       ├── memory/                # Three-tier memory
│       ├── agent/                 # Agent lifecycle
│       ├── router/                # Message routing
│       ├── assistant/             # Assistant agent
│       ├── mcp/                   # MCP tool execution
│       ├── skill/                 # Skill discovery
│       ├── plugin/                # Plugin manager
│       └── error.rs               # Error types
├── crates/tui/                    # TUI terminal (Ratatui)
├── plugins/                       # Node.js plugin host
└── docs/superpowers/              # Design documents
```

## Tests

```bash
cargo test --workspace   # All tests
cargo test --lib          # Unit tests only
cargo test memory::forgetting
cargo test assistant::time_policy
cargo test --test smoke_test
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Core Engine | Rust (Tokio async) |
| TUI | Ratatui + Crossterm |
| LLM | Anthropic Claude / DeepSeek V4 / Ollama |
| Storage | SQLite (sqlx) |
| Vectorization | ONNX Runtime / hash-based fallback |
| Feishu | lark-cli |
| MCP | stdio / HTTP transport, JSON-RPC |
| Plugins | Node.js + JSON-RPC over stdio |
| File Watching | notify (hot-reload) |

## License

[Apache License 2.0](LICENSE)