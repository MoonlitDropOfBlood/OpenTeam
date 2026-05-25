# Feishu Agent Orchestrator

> **在飞书群里开一家"一人公司"。**
>
> 自定义 AI Agent（数字员工），在飞书群中像真实团队一样自动协作。
> 用户（老板）在群里发需求，Agent 们自动拆解任务、互相 @ 协作。

## 架构概览

```
┌─────────────────────────────────────────────────┐
│                  你的电脑（本地）                   │
│                                                    │
│  ┌──────────┐  ┌──────────────────────────────┐  │
│  │   TUI    │  │      Rust Core (独立库)        │  │
│  │  (Ratatui)│  │  ┌──────┐ ┌──────┐ ┌──────┐ │  │
│  └──────────┘  │  │Agent  │ │Memory│ │LLM   │ │  │
│                │  │Manager│ │Store │ │Gateway│ │  │
│                │  ├──────┤ ├──────┤ ├──────┤ │  │
│                │  │Router│ │Feishu│ │Secret│ │  │
│                │  │      │ │Bridge│ │-ary  │ │  │
│                │  └──────┘ └──────┘ └──────┘ │  │
│                └──────────┬───────────────────┘  │
│                           │ IPC (JSON-RPC)        │
│                ┌──────────▼───────────────────┐  │
│                │   Node.js Plugin Host         │  │
│                └──────────────────────────────┘  │
│                           │                       │
└───────────────────────────┼───────────────────────┘
                            │ lark-cli
┌───────────────────────────┼───────────────────────┐
│                           │       飞书平台          │
│                ┌──────────▼───────────────────┐   │
│                │        飞书群聊（协作可见层）     │   │
│                │ 秘书: "PRD 写好了，请过目"      │   │
│                │ 用户: "@Dev 直接按方案 A 做"   │   │
│                └──────────────────────────────┘   │
└──────────────────────────────────────────────────┘
```

## 功能特性

### Agent 系统
- **自定义 Agent**：通过 YAML 定义角色、性格、LLM 配置、触发词
- **秘书 Agent**：主 Agent，忙时主动推进，闲时静默归档，紧急关键词立即唤醒
- **双通道通信**：用户可 @秘书（日常协调）或直接 @任意 Agent（最高优先级）
- **忙闲感知**：周一至周五 9:00-18:00 忙时模式，其余时间闲时模式，紧急关键词覆盖一切

### 记忆系统
- **三层记忆**：Working Memory（LLM 上下文）→ Short-term（结构化摘要）→ Long-term（持久知识）
- **语义检索**：余弦相似度向量搜索
- **自然遗忘**：艾宾浩斯衰减模型，重要性 8-10 近永久保留
- **自动压缩**：短对话规则提取，长对话 LLM 压缩
- **SQLite 存储**：本地持久化，零外部依赖

### LLM 集成
- **多 Provider**：Anthropic Claude + Ollama 本地模型
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

## 快速开始

### 前置条件

- **Rust** 1.75+（[rustup](https://rustup.rs/)）
- **Node.js** 18+（插件系统用）
- **lark-cli**（飞书 CLI，[安装指南](https://open.feishu.cn/document/uYjL24iN/uMDMxEjLzAjMx4yM0ITM)）

### 配置

#### 1. 设置 API 密钥

```bash
export ANTHROPIC_API_KEY=sk-ant-xxx
```

#### 2. 定义 Agent

创建 `agents/my-agent.yaml`：

```yaml
name: "CodeCat"
role: "你是一个资深后端工程师，擅长 Rust 和系统架构设计"
personality: "严谨、有条理、注重代码质量"
llm:
  primary:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    timeout_secs: 120
  fallback:
    provider: anthropic
    model: claude-haiku-3-5-20241022
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 4096
    timeout_secs: 60
triggers:
  - pattern: "开发|实现|代码|架构"
    auto_respond: true
  - pattern: "@Dev|@CodeCat"
    auto_respond: true
skills:
  - feishu-doc
  - web-search
mcps: []
```

#### 3. 配置 LLM 模型池

编辑 `llm_config.yaml`（可选，Agent 已内嵌配置）：

```yaml
models:
  claude-sonnet-4:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }

  ollama-qwen:
    provider: ollama
    model: qwen2.5:3b
    max_tokens: 4096
    timeout_secs: 60
```

### 构建 & 运行

```bash
# 构建全部
cargo build

# 运行测试（49 tests）
cargo test

# 启动 TUI
cargo run -p feishu-agent-tui

# 启动插件宿主
cd plugins/host && node src/index.js
```

## 项目结构

```
├── Cargo.toml                     # Workspace 定义
├── llm_config.yaml                # 全局 LLM 模型池
├── agents/                        # Agent YAML 配置
│   └── pm.yaml                    # 示例：产品经理 Agent
├── crates/
│   ├── core/                      # Rust Core 库
│   │   └── src/
│   │       ├── config/            # Agent/LLM 配置解析
│   │       ├── registry/          # Agent 注册中心
│   │       ├── llm/               # LLM Gateway + 限流
│   │       ├── feishu/            # 飞书 CLI Bridge
│   │       ├── memory/            # 记忆系统（三层）
│   │       ├── agent/             # Agent 生命周期管理
│   │       ├── router/            # 消息路由
│   │       ├── secretary/         # 秘书 Agent
│   │       ├── plugin/            # 插件管理器
│   │       ├── error.rs           # 统一错误类型
│   │       └── lib.rs             # Core 入口
│   └── tui/                       # TUI 终端界面
│       └── src/
│           ├── main.rs            # 事件循环
│           ├── app.rs             # 应用状态
│           ├── ui.rs              # 渲染分发
│           └── pages/             # 各页面组件
├── plugins/                       # 插件系统
│   ├── host/                      # Node.js 宿主
│   │   └── src/
│   │       ├── index.js           # JSON-RPC 主循环
│   │       ├── registry.js        # Hook 注册表
│   │       └── transport.js       # stdio 传输层
│   ├── sdk/                       # 插件 SDK
│   └── examples/                  # 示例插件
└── docs/superpowers/              # 设计文档
```

## Agent YAML 配置参考

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | String | 是 | Agent 名称（用于 @mention） |
| `role` | String | 是 | System Prompt，定义角色行为 |
| `personality` | String | 否 | 性格描述 |
| `llm.primary` | Object | 是 | 主 LLM 配置 |
| `llm.fallback` | Object | 否 | 降级 LLM 配置 |
| `triggers[].pattern` | String | 是 | 触发关键词（正则） |
| `triggers[].auto_respond` | Bool | 否 | 是否自动响应（默认 true） |
| `skills[]` | String[] | 否 | 绑定的 Skill 列表 |
| `mcps[]` | String[] | 否 | 绑定的 MCP 工具列表 |

### ModelConfig 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `provider` | String | 是 | `anthropic` 或 `ollama` |
| `model` | String | 是 | 模型 ID |
| `api_key_env` | String | 否 | 环境变量名（API Key） |
| `max_tokens` | u32 | 是 | 最大输出 token 数 |
| `timeout_secs` | u64 | 否 | API 超时（默认 120s） |
| `rate_limit` | Object | 否 | RPM/TPM 限流配置 |

## 测试

```bash
# 运行所有测试
cargo test --workspace

# 只运行单元测试
cargo test --lib

# 运行特定模块测试
cargo test memory::forgetting
cargo test secretary::time_policy

# 运行冒烟测试
cargo test --test smoke_test
```

## 发展阶段

| 阶段 | 状态 | 内容 |
|------|------|------|
| Phase 1 | ✅ 完成 | Rust Core 脚手架、Agent 配置/注册表、LLM Gateway、飞书 Bridge |
| Phase 2 | ✅ 完成 | 记忆系统（三层 + 遗忘）、Agent 生命周期、消息路由、秘书 Agent |
| Phase 3 | ✅ 完成 | TUI 界面（5 页面）、Plugin 系统（Node.js 宿主 + Hook） |
| 清理优化 | ✅ 完成 | 49 测试覆盖、TUI 连接真实数据、Agent LLM 调用、自动刷新、记忆浏览器 |

### 未来方向

- ONNX Runtime 全量集成（当前使用 stub）
- Ollama 本地压缩（当前使用 rule-based fallback）
- Agent 间真正协作（通过飞书群 @mention）
- Supervisor 熔断器（插件崩溃自动隔离）
- Web 管理控制台

## 技术栈

| 层 | 技术 |
|---|------|
| 核心引擎 | Rust (Tokio async) |
| TUI | Ratatui + Crossterm |
| LLM | Anthropic Claude + Ollama |
| 存储 | SQLite (sqlx) |
| 向量化 | ONNX Runtime (stub) / nomic-embed-text-v1 |
| 压缩 | Ollama / qwen2.5:3b |
| 飞书 | lark-cli (飞书 CLI) |
| 插件 | Node.js + JSON-RPC over stdio |

## 许可证

MIT