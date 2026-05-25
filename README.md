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

| 依赖 | 版本 | 用途 | 安装 |
|------|------|------|------|
| Rust | ≥ 1.75 | 核心引擎 | [rustup](https://rustup.rs/) |
| Node.js | ≥ 18 | 插件宿主 | [nodejs.org](https://nodejs.org/) |
| lark-cli | 最新 | 飞书集成 | [飞书 CLI 指南](https://open.feishu.cn/document/uYjL24iN/uMDMxEjLzAjMx4yM0ITM) |
| Ollama (可选) | ≥ 0.1 | 本地 LLM | [ollama.com](https://ollama.com/) |

### 第一步：克隆 & 构建

```bash
git clone <repo-url>
cd feishu-agent-orchestrator

# 构建全部（首次需要下载依赖，约 2-3 分钟）
cargo build

# 确认构建成功
cargo test
```

### 第二步：配置 Agent

项目自带一个示例 Agent `agents/pm.yaml`（产品经理"小红"）。你可以：

**a) 直接使用默认配置试跑：**
```bash
cargo run -p feishu-agent-tui
```

此时 TUI 会加载 `agents/pm.yaml`，在 Home 页面看到 Agent 卡片。

**b) 创建自己的 Agent：**

在 `agents/` 目录下新建 YAML 文件，系统会自动加载该目录下所有 `.yaml` 文件。

```bash
# 举例：创建一个开发 Agent
touch agents/codecat.yaml
```

`agents/codecat.yaml`：
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
skills: []
mcps: []
```

### 第三步：设置 API 密钥

每个 Agent 可以独立配置 LLM。将 API Key 设为环境变量（名称对应 YAML 中的 `api_key_env`）：

```bash
# Windows PowerShell
$env:ANTHROPIC_API_KEY = "sk-ant-xxx"

# macOS / Linux
export ANTHROPIC_API_KEY=sk-ant-xxx
```

如果使用 Ollama 本地模型，无需 API Key，确保 Ollama 服务运行在 `localhost:11434`：

```bash
ollama pull qwen2.5:3b
ollama serve
```

### 第四步：配置飞书（可选）

如果需要飞书群聊集成：

```bash
# 1. 登录飞书 CLI
lark-cli login

# 2. 验证登录状态
lark-cli auth check

# 3. 配置 WebSocket 事件（在飞书开发者后台开启 im.message.receive_v1）
#    详情参见飞书开放平台文档
```

飞书集成会自动检测连接状态，在 TUI F5 页面可查看。

### 第五步：运行

```bash
# 方式一：TUI 终端界面（推荐）
cargo run -p feishu-agent-tui

# 方式二：仅运行测试
cargo test

# 方式三：启动插件宿主（开发插件时使用）
cd plugins/host && node src/index.js
```

### TUI 使用指南

启动 TUI 后：

```
┌─────────────────────────────────────────────────────┐
│ My Virtual Company    Agents: 1 loaded     Noti: 0  │
├─────────────────────────────────────────────────────┤
│ ┌─ Agents ───────────┐ ┌─ Messages ───────────────┐ │
│ │ 小红 (PM)  🟢 Idle │ │ System ready.             │ │
│ └────────────────────┘ └──────────────────────────┘ │
├─────────────────────────────────────────────────────┤
│ F1:Home  F2:Agents  F3:Tasks  F4:Logs  F5:Feishu  │
│ F6:Memory    r:refresh    a:auto    q:quit          │
└─────────────────────────────────────────────────────┘
```

**操作说明：**
- **F1-F6**：切换页面（Home / Agents / Tasks / Logs / Feishu / Memory）
- **r**：手动刷新数据
- **a**：切换自动刷新（默认开启，每 5 秒刷新）
- **q / Esc**：退出

**各页面内容：**
| 页面 | 看到什么 |
|------|----------|
| **Home (F1)** | 所有 Agent 概览卡片 + 系统消息流 |
| **Agents (F2)** | Agent 详细列表，含角色、状态、技能 |
| **Tasks (F3)** | 任务看板 |
| **Logs (F4)** | LLM 调用日志、CLI 执行记录 |
| **Feishu (F5)** | 飞书连接状态、WebSocket 事件、发送队列 |
| **Memory (F6)** | 各 Agent 的记忆列表（类型、重要性、摘要） |

### 示例工作流

假设你配置了 PM Agent（小红）和 Dev Agent（CodeCat）：

```
1. TUI 启动 → F1 看到两个 Agent 卡片（小红 🟢 Idle, CodeCat 🟢 Idle）
2. 创建一个任务 → F3 添加任务
3. 各 Agent 自动处理 → F4 查看 LLM 调用日志
4. 查看记忆 → F6 可以看到 Agent 自动压缩存储的对话摘要
5. 飞书集成后 → F5 查看飞书消息事件流
```

### 下一步

- [创建更多 Agent](/agents/) — 每个 `.yaml` 文件 = 一个数字员工
- [编写插件](/plugins/examples/) — 用 Node.js 扩展系统功能
- [接入飞书群](#第四步配置飞书可选) — 让 Agent 在飞书里协作

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

[Apache License 2.0](LICENSE)