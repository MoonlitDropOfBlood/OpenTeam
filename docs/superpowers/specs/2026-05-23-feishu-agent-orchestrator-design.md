# Feishu Agent Orchestrator — 设计文档

> 一个基于飞书的 AI Agent 编排平台，让你在飞书群里开一家"一人公司"。
> 用户创建自定义 Agent，Agent 们在飞书群中像真实团队一样自动协作。

---

## 1. 系统概览 & 核心概念

### 1.1 产品定位

**基于飞书的 AI Agent 编排平台。** 用户（老板）在飞书群里发需求，自定义创建的 Agent（员工）自动拆解任务、互相 @ 协作、在群里完成工作。用户随时可见全局、随时插话干预。

### 1.2 核心角色

| 角色 | 说明 |
|------|------|
| **用户（你）** | 在飞书群里发号施令、拍板决策、随时介入 |
| **Core（Rust 独立库）** | 系统中枢。管理 Agent 生命周期、消息路由、LLM 调用、飞书连接、记忆系统。无 UI 依赖 |
| **秘书 Agent（主 Agent）** | 用户与虚拟公司的唯一对话桥梁。分派任务、协调团队、定时汇总。忙时主动推进，闲时静默归档 |
| **Agent** | 用户自定义的"数字员工"。有角色名、能力（Skills）、工具（MCP），背后是 LLM。可直接被用户 @（最高优先级） |
| **Registry（注册中心）** | 所有 Agent 的花名册。记录谁叫什么、有什么技能、当前状态 |

### 1.3 系统边界

```
┌─────────────────────────────────────────────────┐
│              你的电脑（本地）                      │
│                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │
│  │   TUI    │  │   CLI    │  │   Web (V2)   │   │
│  └────┬─────┘  └────┬─────┘  └──────┬───────┘   │
│       │             │               │            │
│  ┌────▼─────────────▼───────────────▼─────────┐  │
│  │              Rust Core（独立库）              │  │
│  │  ┌─────────┐ ┌──────────┐ ┌─────────────┐ │  │
│  │  │  Agent   │ │  Msg     │ │  LLM        │ │  │
│  │  │ Manager  │ │  Router  │ │  Gateway    │ │  │
│  │  └─────────┘ └──────────┘ └─────────────┘ │  │
│  │  ┌─────────┐ ┌──────────┐ ┌─────────────┐ │  │
│  │  │ Memory  │ │ Feishu   │ │  Plugin     │ │  │
│  │  │ System  │ │ Bridge   │ │  Manager    │ │  │
│  │  └─────────┘ └──────────┘ └─────────────┘ │  │
│  └─────────────────────┬───────────────────────┘  │
│                        │ IPC (JSON-RPC)            │
│  ┌─────────────────────▼───────────────────────┐  │
│  │         Node.js Plugin Host (单进程)         │  │
│  │  ┌──────┐  ┌──────┐  ┌──────┐              │  │
│  │  │Plugin│  │Plugin│  │Plugin│              │  │
│  │  │  A   │  │  B   │  │  C   │  ...          │  │
│  │  └──────┘  └──────┘  └──────┘              │  │
│  └──────────────────────────────────────────────┘  │
│                        │ Feishu CLI                 │
└────────────────────────┼───────────────────────────┘
                         │
┌────────────────────────┼───────────────────────────┐
│                        │       飞书平台               │
│  ┌─────────────────────▼───────────────────────┐   │
│  │              飞书群聊（协作可见层）             │   │
│  │  秘书: "老板，PRD 和方案都好了，请过目"          │   │
│  │  用户 @秘书: "让 Dev 直接按方案 A 做"           │   │
│  │  用户 @Dev:  "这个 bug 先修，优先级最高"         │   │
│  └─────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────┘
```

### 1.4 典型工作流

**日常通道（通过秘书）：**
1. 用户在飞书群里 @秘书：「帮我搞定用户注册模块」
2. 秘书分析任务 → 创建话题 #42 → @PM 写 PRD
3. Agent PM 写完 PRD → @Dev 评估方案
4. Agent Dev 评估完毕 → 秘书汇总：「老板，PRD 和方案都好了，请过目」
5. 用户 @秘书：「让 Dev 直接按方案 A 开始」

**老板直连（最高优先级）：**
1. 用户直接 @Dev：「线上有个 bug，优先修」
2. Agent Dev 立刻停下当前任务 → 处理老板指令 → 完成后汇报秘书
3. 秘书知道：「老板直接交办的 X 已完成」

**跨 Agent 协作（Agent 间互 @）：**
1. Agent Dev 在实现中需要 PM 澄清需求 → @PM 协作
2. PM 响应 → 双方完成后汇报秘书

全程飞书群可见，用户随时可插话。秘书是用户与团队的唯一桥梁，但用户作为老板有权直接指挥任何人。

### 1.5 运行模式

| 模式 | 说明 | 阶段 |
|------|------|------|
| TUI | 终端 UI，Agent 管理 + 消息流 + 日志 | V1 |
| CLI | 命令行接口，批量操作 + 脚本集成 | V2 |
| Web | Web 管理控制台 | V2 |

Agent 运行时：
- V1：本地运行（用户电脑）
- V2：支持混合模式（本地 + 云端）

---

## 2. Agent 定义模型

### 2.1 定义结构

```yaml
# agents/pm.yaml
name: "小红"
role: "你是一个资深产品经理，擅长需求分析和PRD撰写"
personality: "严谨、有条理、善于沟通"
llm: "claude-sonnet-4"

triggers:
  - pattern: "需求|PRD|产品文档"
    auto_respond: true
  - pattern: "@PM|@小红"
    auto_respond: true

skills:
  - feishu-doc         # 飞书文档操作
  - feishu-task        # 飞书任务管理

mcps:
  - github             # 代码仓库
```

### 2.2 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | Agent 在飞书群中显示的名称 |
| `role` | string | 系统提示词，定义 Agent 行为和职责 |
| `personality` | string | 性格/语气风格 |
| `llm` | string | 底层模型标识（可插拔） |
| `triggers` | array | 触发规则：关键词模式 + @ 提及，**两者都支持** |
| `skills` | array | 能力包列表（飞书 CLI Skills 或自定义） |
| `mcps` | array | MCP 工具连接配置 |

### 2.3 创建方式

| 方式 | 说明 | 阶段 |
|------|------|------|
| YAML/JSON 配置文件 | 编辑文件定义 Agent，TUI 中加载 | V1 |
| TUI 交互式向导 | 菜单引导：选角色 → 配能力 → 确认启动 | V1 |
| 飞书群创建 | 在群里 @Bot「创建一个叫小红的 PM」 | V2 |

---

## 3. Agent 通信协议

### 3.1 消息通道

| 通道 | V1 作用 | V2 规划 |
|------|---------|---------|
| **飞书群聊通道** | Agent 之间唯一通信渠道。所有协作消息都在飞书群可见 | 不变 |
| **内部通道** | **仅记录/日志**（不参与业务消息路由）。用于 TUI 日志、审计追溯 | 开启真正内部消息传递 |

V1 不存在双通道去重问题——所有业务消息只走飞书群聊。

### 3.2 消息类型

```typescript
// 类型 1：群聊可见消息（飞书群，Agent 间唯一通信渠道）
{
  type: "public_message",
  from: "agent-pm-001",
  to: "agent-dev-002",        // @目标，可为 broadcast
  thread_id: "thread_42",
  content: "@CodeCat PRD已写完，请评估技术方案",
  attachments: ["feishu://doc/xxx"],
  metadata: {
    priority: "high",
    task_id: "task-42"
  }
}

// 类型 2：内部日志消息（仅记录，不参与路由）
{
  type: "internal_log",
  from: "agent-pm-001",
  to: "agent-dev-002",
  content: {
    task_id: "task-42",
    action: "mention_agent",
    timestamp: "2026-05-23T14:30:00Z"
  }
  // 仅写入日志/审计，不触发 Agent 行为
}
```

### 3.3 Agent 互 @ 决策流程

```
Agent 内部决策:
  1. Agent 收到任务 (来自 Router)
  2. LLM 分析: "我需要做 X，但还需要 Y 才能完成"
  3. Agent 查询 Registry: "谁有 Y 相关的能力？"
  4. LLM 决定: "@Agent_B 帮我做 Y"
  5. 在飞书群发 @Agent_B 消息（用户可见，唯一通信渠道）
```

### 3.4 消息优先级

```
用户直接 @Agent（最高） >  紧急关键词  >  秘书分派任务  >  Agent 间协作请求

Agent 的收件箱是优先队列（非 FIFO）:
  1. 用户直接 @ → 插队到最前，当前任务挂起
  2. 紧急关键词 → 次优先级
  3. 秘书分派 → 正常排队
  4. Agent 间互 @ → 正常排队

挂起策略:
  Agent 正在执行的任务 → check point（保存中间状态）
  → 执行老板指令 → 恢复原任务
```

### 3.4 消息流转全图

```
  用户发消息 (@Bot)
        │
        ▼
┌───────────────────┐
│  Feishu CLI Bridge │  ← WebSocket 监听群聊
│  (消息接入层)       │
└───────┬───────────┘
        │ 解析消息
        ▼
┌───────────────────┐
│  Message Router    │  ← Orchestrator 核心
│  - 意图识别        │
│  - 触发匹配        │
│  - 上下文组装      │
└───────┬───────────┘
        │ 路由到目标 Agent(s)
        ▼
┌───────────────────┐     ┌───────────────────┐
│  Agent A (活跃)    │────▶│  Agent B (被@)     │
│  执行任务          │     │  收到协作请求       │
└───────┬───────────┘     └───────┬───────────┘
        │ 发消息到群              │ 发消息到群
        ▼                         ▼
┌───────────────────────────────────────┐
│           飞书群聊                      │
│  Agent A: PRD已写完 @Agent B 请评估     │
│  Agent B: 收到，我来看看...             │
│  Agent B: 方案评估通过，建议JWT+Redis   │
└───────────────────────────────────────┘
```

---

## 4. Agent 记忆系统

> 没有记忆的 Agent 就像每天失忆的员工——上下文再长也不够用。
> 记忆系统提供三层存储 + 自然遗忘，让 Agent 真正拥有"经验"。

### 4.1 三层记忆架构

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  Working      │ ──▶│  Short-term   │ ──▶│  Long-term    │
│  Memory       │    │  Memory       │    │  Memory       │
│ (LLM上下文)   │    │ (当前任务)    │    │ (持久知识)    │
│              │    │               │    │              │
│ ~200K tokens │    │ 1-30天的对话  │    │ >30天的重要   │
│ 即时读写      │    │ 结构化摘要     │    │ 知识+经验      │
│              │    │ 语义检索       │    │ 向量化存储     │
└──────────────┘    └──────────────┘    └──────────────┘
     ▲                    ▲                    │
     │                    │                    │
     └──────── 压缩 ──────┘          ┌─────────▼──────────┐
                                     │  自然遗忘           │
                                     │  - 时间衰减          │
                                     │  - 重要性加权        │
                                     │  - 容量淘汰          │
                                     └────────────────────┘
```

### 4.2 各层职责

#### Working Memory（工作记忆）

- **作用**: Agent 当前对话窗口，直接注入 LLM prompt 上下文
- **生命周期**: 单次对话，对话结束即清空
- **容量**: 取决于 LLM 上下文窗口（~200K tokens）
- **内容**: 原始对话消息、工具调用结果、当前任务上下文

#### Short-term Memory（短期记忆）

- **作用**: 当前任务/项目周期内的结构化记忆
- **生命周期**: 1-30 天（任务完成后自然降级）
- **存储**: 结构化摘要 + 嵌入式向量
- **检索**: 语义搜索，Agent 遇到相关问题时自动召回
- **内容**: 对话压缩后的结构化摘要（谁、何时、做了什么决定、产出是什么）

```yaml
# 示例：一条短期记忆
id: mem_2026_0523_001
type: decision
title: "用户注册模块技术方案确定"
summary: "团队决定采用 JWT + Redis 方案，放弃 Session 方案"
context:
  task_id: "task-42"
  agents: ["小红(PM)", "CodeCat(Dev)"]
  feishu_doc_ref: "feishu://doc/xxxx"
  decision_reason: "性能更好，支持分布式部署"
importance: 8  # 1-10，人工 + 自动评定
created_at: 2026-05-23T14:30:00Z
last_accessed: 2026-05-23T16:00:00Z
```

#### Long-term Memory（长期记忆）

- **作用**: 跨项目/跨周期的持久化知识和经验
- **生命周期**: >30 天，重要条目可永久保留
- **存储**: 向量数据库（语义检索）+ 结构化索引
- **内容**: 经二次提炼的"经验"——去噪、关联、抽象后的知识

### 4.3 记忆流转过程

```
  完整对话
    │
    ▼
[压缩] Agent 自身 或 秘书 Agent 自动将对话压缩为结构化摘要
    │  - 提取关键信息: 谁说了什么、做了什么决定
    │  - 丢弃闲聊噪声
    │  - 生成嵌入式向量
    ▼
┌─────────────────┐
│ Short-term Memory│ ← 存入，可语义检索
└────────┬────────┘
         │ 超过 30 天阈值
         ▼
[二次提炼] 去噪、关联、抽象
    │  - 合并重复/相似的记忆
    │  - 抽取通用模式
    │  - 建立知识图谱关联
    ▼
┌─────────────────┐
│ Long-term Memory │ ← 永久存储，Agent 的"经验库"
└─────────────────┘
```

### 4.4 压缩策略

#### 压缩引擎：本地模型优先

```
向量化（Embedding）              压缩/摘要（Compression）
┌────────────────────┐          ┌──────────────────────┐
│ ONNX Runtime        │          │ Ollama（本地）         │
│ nomic-embed-text-v1 │          │ qwen2.5:3b 或 phi4    │
│ 137M params         │          │ 3-4B params           │
│ 768维向量            │          │ 零 API 成本            │
│ ~500MB 磁盘          │          │ ~2-3GB 显存/内存       │
└────────────────────┘          └──────────────────────┘

回退策略:
  本地 Ollama 不可用 → 远程轻量模型（Claude Haiku / GPT-4o-mini）
```

#### 分级压缩

| 对话规模 | 压缩方式 | 说明 |
|----------|----------|------|
| 短对话（<10轮） | 模板化提取（规则，不用 LLM） | 提取参与者列表、产出链接、待办事项 |
| 中等对话（10-50轮） | 本地 Ollama 轻量模型 | 成本为零，延迟低 |
| 长对话（>50轮） | 本地 Ollama 或远程全量模型 | 重要对话才用全能力模型 |

#### 压缩可靠性

```
压缩流程:
  原始对话
    ├── Step 1: LLM 生成压缩摘要（结构化 JSON）
    ├── Step 2: Schema 校验
    │     - 必填字段: title, decisions[], artifacts[], pending_todos[]
    │     - decisions 每项必须含 reason（不能只有结论没理由）
    │     - 校验失败 → 再试一次（换温度参数）
    │     - 再失败 → 保留原始 + 标注 "压缩失败"
    ├── Step 3: 原始+摘要双存（不可逆操作前保留原始）
    │     - raw/     ← 完整原始对话，不做任何剪裁
    │     - summary/ ← 结构化摘要 + turn_indices（可回溯原文）
    │     - raw 保留策略: raw_retention_days: 7（摘要验证通过后删除）
    │     - 可选: 24h 以上的 raw 文件自动 gzip 压缩归档
    └── Step 4: 检索时摘要命中 → 可选回溯原始上下文展开细节
```

#### 压缩触发时机

- 对话结束（话题沉寂超过 5 分钟）
- 用户主动说「总结一下」
- 单次对话超过 50 轮
- 接近 LLM 上下文窗口上限

### 4.5 记忆检索

Agent 在接到新任务时，自动检索相关记忆：

```
检索流程:
  1. 提取任务关键词 → 生成查询向量
  2. 在 Short-term Memory 中语义搜索 (优先级高)
  3. 在 Long-term Memory 中语义搜索 (补充)
   4. 按 (语义相关度 × (1+boost) × e^(-λ×t)) 排序
  5. Top-N 记忆注入 LLM 上下文
```

**Agent 主动回忆:**
- Agent 可以在 LLM 推理过程中决定"我需要查一下之前的类似情况"
- 通过 MCP 工具 `memory:search` 主动检索
- 秘书 Agent 也会在任务分派时自动附带相关记忆

### 4.6 自然遗忘（艾宾浩斯模型）

```
记忆强度
  │
1.0│█
   │ ██
   │   ██        ← 关键点：重要性 > 阈值则永久保留
0.8│     ██
   │       ██
0.5│         ████       ← 普通点：指数衰减
   │               ████
0.2│                     ██████████
   │
0.0│________________________________▶ 时间
     1天   1周    1月     3月     6月
```

**衰减公式：**

```
当前记忆强度 = 初始强度 × e^(-λ × t)

其中:
  λ = 衰减速率，取决于重要性级别:
    - 重要性 8-10: λ = 0.001 (几乎不衰减，接近永久)
    - 重要性 5-7:  λ = 0.01  (缓慢衰减，数月后模糊)
    - 重要性 1-4:  λ = 0.05  (快速遗忘，适合短期信息)

  t = 距创建时间（天）  ← 关键：基于创建时间，非上次访问时间
```

**检索排序 vs 衰减（解耦）：**

```
检索排序分 = 语义相关度 × (1 + boost) × e^(-λ × t)

  boost     = min(检索次数 × 0.05, 1.0)   ← 检索多的排在前面
  e^(-λ×t)  只跟创建时间有关               ← 时间到了该忘还忘
  importance 只由人工+LLM评估一次，不变        ← 不因检索提升

  效果: 被检索多的记忆 "容易被找到" 但不等于 "永远不死"
```

**重要性评定：**

| 来源 | 方式 | 说明 |
|------|------|------|
| 人工标注 | 用户对 Agent 说「记住这个」/「这个很重要」 | 设为 importance=10 |
| 自动评估 | Agent LLM 评估事件重要程度（一次性评估） | 涉及核心目标、用户强调、重大决策 → 高分 |

**淘汰策略（容量上限触发）：**

```
当记忆库容量接近上限时:
  1. 计算每条记忆的"保留价值" = 重要性 × e^(-λ × t) × (1 + 检索次数 × 0.1)
  2. 升序排序
  3. 淘汰末尾 10%（先压缩 → 再删除）
  4. 保留的条目保留最终摘要（不会无声消失）
```

**可配置参数（TUI 中调节）：**

```yaml
memory:
  short_term:
    max_age_days: 30          # 短期记忆最大保留天数
    max_count: 500            # 短期记忆最大条数
    compress_threshold: 0.8   # 占用超过 80% 时触发压缩
  long_term:
    max_count: 2000           # 长期记忆最大条数
    retention_importance: 7   # 重要性 >= 7 则永久保留
  forgetting:
    base_decay_rate: 0.01     # 基础衰减速率 λ
    retrieval_boost: 0.1      # 每次检索提升的权重
```

### 4.7 记忆可见性

用户可以在 TUI 中查看任意 Agent 的记忆：

```
┌─ 小红 (PM) — 记忆 ───────────────────────────┐
│ [短期] 共 12 条    [长期] 共 3 条               │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ [S][高] 2026-05-23  用户注册技术方案确定         │
│         JWT+Redis，放弃Session方案              │
│         强度: 0.95 | 检索: 3次                   │
│ ─────────────────────────────────────────── │
│ [S][中] 2026-05-22  首页性能问题定位            │
│         发现N+1查询瓶颈，需优化ORM               │
│         强度: 0.72 | 检索: 1次                   │
│ ─────────────────────────────────────────── │
│ [L][高] 2026-04-15  项目架构惯例: 所有API用RESTful│
│         强度: 0.99 | 检索: 8次 | 永久保留         │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
│ [置为重要] [手动遗忘] [导出]                      │
└─────────────────────────────────────────────┘
```

### 4.8 技术实现

| 组件 | 技术选型 |
|------|----------|
| 向量化 | ONNX Runtime + nomic-embed-text-v1（本地，137M params） |
| 压缩/摘要 | Ollama + qwen2.5:3b 或 phi4（本地，零成本）；回退：远程轻量模型（Haiku/GPT-4o-mini）|
| 短期记忆存储 | SQLite + 向量扩展 (sqlite-vec) + 原始对话 JSON 文件 |
| 长期记忆存储 | SQLite（V1）/ Qdrant 或 LanceDB（V2） |
| LLM 上下文注入 | 检索 Top-5 相关记忆，注入 System Prompt |
| 记忆 Schema 校验 | JSON Schema 验证压缩输出，失败重试 + 降级标记 |

---

## 5. LLM Runtime

Agent 的一切智能行为依赖 LLM 调用。本章定义 LLM 如何接入、如何管理、如何与工具系统集成。

### 5.1 中央 LLM Gateway（Rust 侧）

```
┌──────────────────────────────────────────────────┐
│              LLM Gateway (Rust)                   │
│                                                    │
│  ┌──────────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Model Registry│  │ Rate      │  │ Retry        │ │
│  │ (name→config) │  │ Limiter   │  │ Engine       │ │
│  └──────┬───────┘  └────┬─────┘  └──────┬──────┘ │
│         │               │               │         │
│  ┌──────▼───────────────▼───────────────▼──────┐  │
│  │           HTTP Client (reqwest)              │  │
│  │    - Anthropic API / OpenAI API / Ollama     │  │
│  │    - SSE Streaming → channel → TUI           │  │
│  │    - Token counting (tiktoken-rs)            │  │
│  └─────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
         ▲                              ▲
         │ Rust Plugin API             │ IPC: llm:chat
         │ (sync)                      │ (JSON-RPC)
    ┌────┴──────┐              ┌───────┴───────┐
    │ Built-in   │              │  Node.js      │
    │ Plugins    │              │  Plugins      │
    └───────────┘              └───────────────┘
```

**设计决策：Rust 中央统一调用，插件不直接访问 LLM API。**

| 决策 | 理由 |
|------|------|
| Rust 统一持有 API Key（env var / config 文件） | 安全集中管理，插件无权访问密钥 |
| Rust 统一限流（跨 Agent 共享配额） | 多 Agent 并发时避免 429 |
| Rust 统一重试（指数退避 + 降级链） | 一个地方配置，全局生效 |
| 插件通过 JSON-RPC `llm:chat` 调 LLM | 插件无需集成 LLM SDK |

### 5.2 Model 配置

```yaml
# ~/.my-company/llm_config.yaml
models:
  claude-sonnet-4:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    fallback: claude-haiku-3-5
    rate_limit: { rpm: 50, tpm: 100000 }
    timeout: 120s  # LLM 调用全局超时，超时后返回降级错误给 Agent
    
  gpt-4o:
    provider: openai
    model: gpt-4o
    api_key_env: OPENAI_API_KEY
    max_tokens: 8192
    fallback: gpt-4o-mini
    
  ollama-qwen:
    provider: ollama
    model: qwen2.5:3b
    endpoint: http://localhost:11434/v1
    max_tokens: 4096
    # 本地模型，无限流
```

### 5.3 Tools + MCP 模型（对标 OpenCode）

```
Tools（全局，一份，所有 Agent 共享）
  ├── feishu:send_message      飞书发消息
  ├── feishu:create_doc        创建飞书文档
  ├── feishu:read_doc          读飞书文档
  ├── feishu:create_base_record  多维表格写入
  ├── memory:search            记忆语义检索
  ├── registry:query           查注册中心（Agent 发现）
  └── task:create / task:update  任务管理

MCP Servers（所有 MCP 子进程由 Node.js Plugin Host 管理）
  ├── 全局 MCP（所有 Agent 可用）
  │   ├── github        → ~/.my-company/mcp-configs/github.json
  │   └── web-search    → ~/.my-company/mcp-configs/web-search.json
  │
  └── Agent 专属 MCP（agent.yaml 中配置）
      ├── dev.yaml:     mcps: [postgresql, redis]
      └── qa.yaml:      mcps: [test-runner]
```

### 5.4 Agent 调用 LLM 流程

```
Agent 收到任务
  │
  ├── 1. 组装 function_definitions
  │      = Tools (全局) + MCP (全局) + MCP (Agent 专属) + memory:search
  │
  ├── 2. 注入记忆上下文
  │      = 检索 Top-5 相关记忆 → 注入 System Prompt
  │
  ├── 3. LLM Gateway.chat(agent_id, request) → LLM API
  │      - 全局限流检查
  │      - 带退避的重试
  │      - 全局超时 (model.timeout, 默认 120s)
  │      - 超过超时 + fallback 全挂 → 返回降级错误给 Agent
  │
  ├── 4. LLM 返回 tool_call
  │      → Rust Core 执行 tool（直接调 feishu bridge / MCP runtime）
  │      → 结果回灌 LLM
  │      → 循环直到 LLM 返回最终文本
  │
  └── 5. 流式输出（SSE）
         → TUI 消息面板实时渲染
```

### 5.5 密钥与安全管理

| 原则 | 实现 |
|------|------|
| 密钥集中存储 | 环境变量或 `llm_config.yaml`（文件权限 600） |
| 插件无权访问密钥 | Node.js 插件通过 IPC `llm:chat` 调 LLM，不直接拿 key |
| 本地模型（Ollama）优先 | 记忆压缩等高频操作走本地，省成本、少暴露 |
| Token 用量追踪 | tiktoken-rs 计数，按 Agent 统计，TUI 可查看 |

### 5.6 启动预热序列

系统启动时执行以下预热步骤，避免首次 LLM 调用或压缩卡死：

```
启动 → 1. ONNX 加载 (nomic-embed-text-v1)
          └── 若 ~/.cache/ 已有模型 → 加载到内存 (≈500ms)
          └── 若无 → 开始下载 (30-60s)，同时设 embedding_ready=false
              └── 下载完成前 → 禁止记忆检索，Agent 纯推理工作正常

      2. Ollama 探测
          └── GET http://localhost:11434/api/tags (timeout: 500ms)
          └── 有响应 → 检查 qwen2.5:3b/phi4 是否已拉取
              └── 未拉取 → 设 ollama_ready=false，压缩走远程 fallback
          └── 无响应 → 设 ollama_ready=false，压缩走远程 fallback

      3. 飞书 CLI 连通性检查
          └── lark-cli auth check → 确认授权未过期

      4. WebSocket 事件订阅
          └── lark-cli event +subscribe --event-types im.message.receive_v1
```

预热完成后在 TUI 状态栏显示各组件就绪状态。预热失败不影响核心功能（Agent 仍可工作，只是记忆系统暂走 fallback）。

### 5.7 并发模型

**运行时：Tokio async。**

```
  tokio::main
    ├── Agent Manager
    │     ├── Agent A → tokio::task::JoinHandle
    │     ├── Agent B → tokio::task::JoinHandle
    │     └── Agent C → tokio::task::JoinHandle
    │
    ├── WebSocket Event Hub (broadcast channel → fan-out)
    │
    └── 跨 Agent 通信
          ├── Agent ↔ Agent: MessageRouter → mpsc::Sender<AgentCommand>
          ├── Agent ↔ Feishu: 通过 Feishu Bridge
          └── Agent ↔ LLM: 通过 LLM Gateway（共享 reqwest 连接池）
```

**Agent 生命周期：**

```rust
struct AgentHandle {
    id: AgentId,
    join_handle: JoinHandle<()>,           // 可取消
    control_tx: mpsc::Sender<AgentCommand>, // 外部控制通道
    cancel_token: CancellationToken,        // 优雅停止
}

enum AgentCommand {
    Stop, Pause, Resume,
    InjectMessage(Message),                // 来自其他 Agent / Feishu / 用户
    OverrideContext(String),               // 用户强制覆盖上下文
}
```

**Agent 内部事件循环：**

```rust
async fn agent_loop(config, mut control_rx, event_rx, cancel) {
    let mut state = AgentState::Idle;
    let mut inbox: PriorityQueue<Message>;  // 优先队列（非 FIFO）

    loop {
        tokio::select! {
            // 1. 控制指令（优先级最高）
            Some(cmd) = control_rx.recv() => handle_command(cmd),
            // 2. 飞书事件
            Ok(event) = event_rx.recv() => enqueue_event(event),
            // 3. 超时检查（心跳、死锁检测）
            _ = sleep(Duration::from_secs(30)) => check_timeouts(),
            // 4. 取消信号
            _ = cancel.cancelled() => { graceful_shutdown(); break; }
        }
        // 处理优先队列中的消息
        process_inbox(&mut inbox, &config).await;
        // 注意: process_inbox 可能因 LLM 调用阻塞 30s+
        // 每处理完一条消息后应检查 control_rx.try_recv() 避免头部阻塞
    }
}
```

**Agent 等待超时 + 升级：**

```
Agent A @Agent B → 进入 Waiting 状态
  │
  ├── 收到回复（正常）→ 继续
  │
  ├── 第 1 次超时: Router 重试
  ├── 第 2 次超时: 找同角色备选 Agent
  └── 第 3 次超时: 升级给秘书 → 秘书提醒用户
```

**共享状态（Arc + RwLock）：**

```rust
struct SharedState {
    registry: Arc<RwLock<Registry>>,     // Agent 注册表
    memory: Arc<MemoryStore>,            // 记忆存储
    feishu: Arc<FeishuBridge>,           // 飞书 CLI（内部已并发控制）
    llm: Arc<LlmGateway>,               // LLM 连接池（全局限流）
}
```

**优雅停机：**

```
用户按 Q
  → 广播 CancellationToken 给所有 Agent
  → 每 Agent: 完成当前 LLM 调用 → 发最后一条飞书消息 → 退出
  → 等待所有 JoinHandle（最大 30 秒）
  → 强制 kill 超时未结束的 Agent
  → Flush 日志 → 退出
```

---

## 6. 消息架构：话题 + 收件箱

### 6.1 问题

多 Agent 在一个群聊里消息密度太高，可能遗漏重要信息。

### 6.2 解决方案组合

```
飞书群            ← 话题线程（任务隔离）
                  ← 秘书 Agent 定时摘要

TUI               ← Agent Inbox（任务聚焦，不漏消息）
                  ← 全局消息流（完整上下文，需要时查看）
```

### 6.3 飞书话题（Thread）驱动

```
主群频道（重要公告 + 新任务入口）
  ├── [话题] 任务 #42: 用户注册模块
  │     Agent PM: PRD已写好 → @Agent Dev
  │     Agent Dev: 收到，评估中...
  │     Agent Dev: @Agent QA 请准备用例
  │     Agent QA: 好，3个核心场景已列出
  │
  ├── [话题] 任务 #43: 首页性能优化
  │     Agent Dev: 发现N+1查询问题
  │     ...
  │
  └── [话题] 任务 #44: 数据看板需求
        Agent PM: 需求确认中...
```

- 每个任务 = 一个飞书话题线程。已通过 `--reply-in-thread` 验证可行
- Agent 在话题内协作，主群频道保持干净
- @mention 格式: `<at user_id="ou_xxx">名称</at>` 嵌在消息文本中

### 6.4 秘书 Agent（主 Agent + 忙闲感知）

秘书是用户与整个虚拟公司的**唯一对话桥梁**——但不是唯一入口。用户作为老板，有权绕过秘书直接 @ 任何 Agent（最高优先级）。

```
                    ┌─────────────────────┐
                    │       用户（老板）     │
                    └──────┬──────┬───────┘
                           │      │
               @秘书       │      │  @任意Agent（最高优先级）
              （日常协调）   │      │
                           ▼      ▼
              ┌──────────────────────────┐
              │        秘书 Agent         │
              │  - 忙时主动推进            │
              │  - 闲时静默归档            │
              │  - 分派 / 协调 / 汇总      │
              └──────┬───────────────────┘
                     │ 分派
         ┌───────────┼───────────┐
         ▼           ▼           ▼
      ┌─────┐    ┌─────┐    ┌─────┐
      │ PM  │    │ Dev │    │ QA  │   ...
      └─────┘    └─────┘    └─────┘
         ▲           ▲           ▲
         │           │           │
         └───────────┴───────────┘
              用户直接 @（最高优先级）
```

#### 忙闲感知

```yaml
secretary:
  time_policy:
    busy:     # 工作日 9:00-18:00
      wake_mode: "proactive"
      summary_interval: "15min"
      escalation_timeout: "10min"       # Agent 阻塞 10 分钟就提醒
      notification: "push"
      
    idle:     # 晚上 / 周末 / 节假日
      wake_mode: "passive"
      summary_interval: "6h"            # 汇总后一次推
      escalation_timeout: "2h"          # 非紧急不打扰
      notification: "digest"
      
    urgent:   # 紧急规则（覆盖忙闲）
      keywords: ["紧急", "线上故障", "P0", "crash"]
      wake_mode: "immediate"

time_source:
  primary: "feishu_calendar"    # 飞书日历自动感知工作时间、假期
  override: "tui_manual"        # 用户可在 TUI 手动切换忙/闲/休假
  fallback: "fixed_config"      # 兜底：Mon-Fri 9:00-18:00
```

#### 忙时 vs 闲时行为对比

| 场景 | 忙时（周二 14:00） | 闲时（周六 22:00） |
|------|-------------------|---------------------|
| Agent Dev 阻塞 | 10分钟后提醒用户 | 2小时后再提醒 |
| Agent PM 完成 PRD | 立刻推送到群 | 记入摘要，下次汇总时一起发 |
| 用户「帮我写个功能」 | 立刻分派 | 立刻分派，但告知"预计明早完成" |
| 用户「紧急！线上挂了」 | 立刻唤醒所有 Agent | **立刻唤醒**（覆盖忙闲） |
| 定时摘要 | 每 15 分钟 | 每 6 小时 |

#### 秘书介入规则

```
场景 1：用户 @Dev "修复这个 bug"
  → Dev 直接处理，秘书不介入
  → Dev 完成后通知秘书：「老板直接交办的 X 已完成」

场景 2：Agent 协作遇到阻塞
  → Dev @PM 没收到回复
  → 忙时 10min / 闲时 2h → 秘书提醒用户

场景 3：用户直连的 Agent 迟迟没完成
  → 忙时 10min / 闲时 2h → 秘书提醒：「老板，您交代的 X 还没完成，需要催吗？」
```

### 6.5 TUI Agent Inbox

```
┌─ TUI Agent Inbox ──────────────────────────┐
│ Agent Dev (CodeCat)                    ⏳忙碌 │
│ ───────────────────────────────────────── │
│ [🔴高优] PM @你: PRD评估 - 用户注册模块      │
│          → 飞书文档: /doc/xxxx               │
│          → 截止: 今天 18:00                   │
│ [🟡普通] QA @你: 测试用例需要接口定义          │
│ [🟢低优] 秘书: 日报待填写                      │
│ ───────────────────────────────────────── │
│ 已处理: 3条 │ 待处理: 2条 │ 搁置: 0条         │
└───────────────────────────────────────────┘
```

---

## 7. Feishu CLI 集成层

### 7.1 飞书 CLI 能力矩阵 (V1)

| 业务域 | 核心命令 | V1 用途 |
|--------|----------|---------|
| 消息与群组 | `im +messages-send` / `im +messages-reply --reply-in-thread` / `im +chat-messages-list` | Agent 收发群消息、话题回复、读历史 |
| 事件监听 | `event +subscribe --event-types im.message.receive_v1` | WebSocket 实时消息接收 |
| 云文档 | `docx +document-create` / `+document-get` / `+document-patch` | Agent 写 PRD、方案文档 |
| 多维表格 | `base +record-create` / `+record-list` | 任务跟踪表、Agent 注册表 |
| 任务 | `task +task-create` / `+task-update` | 拆解和跟踪子任务 |
| 日历 | `calendar +event-create` | 设置里程碑节点 |
| 通讯录 | `contact +user-search` | Agent 查找飞书用户 |

### 7.2 集成架构

```
┌─────────────────────────────────────────────┐
│            Feishu CLI Bridge                 │
│                                               │
│  ┌─────────────┐  ┌──────────────┐           │
│  │ 消息监听器    │  │ 命令执行器     │           │
│  │ (WebSocket)  │  │ (子进程调用)   │           │
│  └──────┬───────┘  └──────┬───────┘           │
│         │                 │                   │
│         └────────┬────────┘                   │
│         ┌────────▼────────┐                   │
│         │  消息解析/序列化  │                   │
│         └────────┬────────┘                   │
└──────────────────┼──────────────────────────┘
                   │
          ┌────────▼────────┐
          │  Orchestrator    │
          └─────────────────┘
```

### 7.3 消息监听 (WebSocket)

```
监听事件:
  - im.message.receive_v1        群聊新消息
  - im.message.reaction.created  表情回应（快速确认）
  - im.chat.member.user.added    新成员入群

处理流程:
  事件到达 → 解析消息体 →
    ├── @Bot 命令 → Router 意图识别 → 分派 Agent
    ├── Agent 间协作消息 → Router 上下文分析 → 推给相关 Agent
    └── 普通闲聊 → 忽略（或记录为上下文背景）
```

### 7.4 命令执行（统一接口）

Agent 不直接调用飞书 CLI 命令，而是通过 Bridge 的语义化接口：

```typescript
await feishu.sendMessage({
  chat_id: "群ID",
  thread_id: "话题ID",
  content: "@CodeCat PRD已写好，请评估",
  mentions: ["agent-dev-001"],
  attachments: [{ type: "doc", url: "feishu://doc/xxx" }]
});
// Bridge 内部翻译为: lark message send --receive-id "群ID" --msg-type text --content "..."
```

**为什么不让 Agent 直接调 CLI？**
- 统一参数校验和错误处理
- 避免 Agent LLM 幻觉生成不存在的命令
- 可合并、去重、节流高频调用

### 7.5 Agent 飞书身份方案

| 阶段 | 方案 |
|------|------|
| **V1** | 一个 Bot 代表所有 Agent，消息内容加身份标识。增删 Agent → 更新 TUI 注册表 |
| **V2** | 每个 Agent = 独立飞书 Bot。创建 Agent → 自动创建 Bot；删除 Agent → 自动删除 Bot（"小红 退出了群聊"） |

### 7.6 发送队列（应对速率限制）

飞书 API 对同一群聊有 **5 QPS** 限制（群内所有 Bot 共享）。当多个 Agent 同时活跃时极易触发。

```
Agent A ──→ ┌──────────────────────────┐
Agent B ──→ │   Message Send Queue      │
Agent C ──→ │   (优先级有序)              │  ──→ Feishu CLI (max 5/sec)
            │                            │       │
            │  优先级:                    │       ├── 429 → 指数退避重试
            │    1. 用户直连 @ 消息        │       └── 连续限流 → 合并相邻消息
            │    2. 紧急关键词消息          │
            │    3. 秘书分派消息            │
            │    4. Agent 间协作消息        │
            │                            │
            │  优化:                      │
            │    - 同话题相邻消息自动合并    │
            │    - 低优先级消息延迟批处理    │
            └──────────────────────────┘
```

- 队列满时：低优先级消息延迟（最多 5 秒），超时后降级发送
- 连续触发 429 → 自动合并队列中同话题的相邻消息为一条

**发送超时分级（每级硬超时，避免无限排队）：**

| 优先级 | 最大排队时长 | 超时后行为 |
|--------|------------|-----------|
| 用户直连 @ | 1s | 强制发送（触发 429 则由退避处理） |
| 紧急关键词 | 2s | 强制发送 |
| 秘书分派 | 5s | 合并为一条批量消息发送 |
| Agent 间协作 | 8s | 丢弃 + 日志警告（不影响 Agent 内部重试） |

### 7.7 WebSocket 事件预配置

飞书 CLI 的 WebSocket 监听需要**预先在飞书开发者后台配置事件**，不能运行时动态订阅。

**用户需在飞书开放平台控制台开启的事件：**

| 事件 | 用途 | 所需权限 |
|------|------|---------|
| `im.message.receive_v1` | 接收群聊消息 | `im:message:receive_as_bot` |
| `im.message.reaction.created_v1` | 表情回应（快速确认） | `im:message:receive_as_bot` |
| `im.chat.member.bot.added_v1` | Bot 被拉入群 | `im:chat:readonly` |

**约束：**
- 仅**自建应用**支持 WebSocket 长连接（不支持商店应用）
- 每个应用默认只有**一个** `+subscribe` 实例（与 Rust Core 单进程架构匹配）
- 事件可能重复推送，需按 `message_id` 做幂等去重
- 连接断开后自动重连

### 7.8 @mention 格式

```bash
# 提及特定用户
lark-cli im +messages-send --chat-id oc_xxx \
  --text '<at user_id="ou_xxx">小红</at> PRD 已写好，请评估'

# @all
<at user_id="all"></at>
```

Agent 间互 @ 通过消息文本中的 `<at user_id="...">名称</at>` 实现。

**V1 注意：** 由于 V1 使用单一 Bot 代表所有 Agent，@mention 中的 `user_id` 指向同一个 Bot。消息在群聊中**视觉上**显示为 @提及，但**不会触发飞书的通知系统**（因为 Bot 不能 @自己）。Agent 间的消息路由由 Rust Core 的 Router 完成，不依赖飞书的 @通知机制。V2 每个 Agent 拥有独立 Bot 时，@mention 恢复正常通知行为。

---

## 8. TUI 设计

### 8.1 技术选型

- **语言/框架**: Rust + Ratatui
- **运行环境**: 本地终端

### 8.2 主界面布局

```
┌──────────────────────────────────────────────────────┐
│  🏢 我的虚拟公司          Agents: 3/3 运行中   🔔 2 条  │  ← 顶部状态栏
├──────────────────────────────────────────────────────┤
│                                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │  ← Agent 卡片区
│  │   小红       │  │  CodeCat    │  │  测试小蓝    │   │
│  │   产品经理    │  │   开发者     │  │   QA 工程师  │   │
│  │   ━━━━━━━━━  │  │   ━━━━━━━━━  │  │   ━━━━━━━━━  │   │
│  │ 🟢 运行中    │  │ 🟡 忙碌      │  │ 🟢 空闲      │   │
│  │ 任务: 写PRD  │  │ 任务: 评估   │  │ 待机中...    │   │
│  └─────────────┘  └─────────────┘  └─────────────┘   │
│                                                       │
├──────────────────────────────────────────────────────┤
│  📋 实时消息流                      [1] 全局 [2] 小红  │  ← 消息面板
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│  14:32  [小红 PM]     PRD已写完，@CodeCat 请评估       │
│  14:33  [CodeCat Dev] 收到，我先看看需求               │
│  14:34  [系统]         Agent CodeCat 状态: 忙碌        │
│  ⚠ 14:35  [秘书]      任务#42 已阻塞 — 等待CodeCat回复│
│                                                       │
├──────────────────────────────────────────────────────┤
│  F1:帮助  F2:Agent  F3:任务  F4:日志  F5:飞书  Q:退出 │  ← 快捷键栏
└──────────────────────────────────────────────────────┘
```

### 8.3 核心页面

| 快捷键 | 页面 | 内容 |
|--------|------|------|
| F1 | 帮助 | 快捷键速查、使用指引 |
| F2 | Agent 管理 | 创建/启停/编辑 Agent、查看配置 |
| F3 | 任务看板 | 所有进行中的任务及状态 |
| F4 | 运行日志 | LLM 调用详情、CLI 命令执行日志 |
| F5 | 飞书状态 | 飞书 CLI 连接状态、WebSocket 事件流 |

### 8.4 Agent 管理页 (F2)

```
┌──────────────────────────────────────────────────────┐
│  👥 Agent 管理                                  + 新建 │
├──────────────────────────────────────────────────────┤
│  ◉ 小红 (PM)            🟢运行中  [停用] [编辑] [删除] │
│    Skills: feishu-doc, feishu-task                   │
│    MCPs: github                                      │
│                                                       │
│  ◉ CodeCat (Dev)        🟡忙碌    [停用] [编辑] [删除] │
│    Skills: feishu-doc, web-search                    │
│    MCPs: github, postgresql                          │
│                                                       │
│  ○ 测试小蓝 (QA)         🟢空闲    [启用] [编辑] [删除] │
│    Skills: feishu-base, web-search                   │
│    MCPs: —                                           │
└──────────────────────────────────────────────────────┘
```

---

## 9. 插件与 Hook 系统

### 9.1 整体分层

```
┌─────────────────────────────────────────────────────┐
│                  Rust Core（独立库）                   │
│  ┌──────────────┐ ┌───────────┐ ┌───────────────┐   │
│  │ Agent Manager │ │ Msg Router │ │ LLM Gateway   │   │
│  └──────┬───────┘ └─────┬─────┘ └───────┬───────┘   │
│         │               │               │           │
│  ┌──────▼───────────────▼───────────────▼───────┐   │
│  │          Plugin Manager (Rust)                │   │
│  │  - 单进程管理 / IPC / 生命周期                 │   │
│  │  - Supervisor: 健康检查 + 崩溃重启 + 熔断     │   │
│  └──────┬───────────────────────────────────────┘   │
└─────────┼───────────────────────────────────────────┘
          │ IPC (stdio, JSON-RPC 2.0)
┌─────────▼───────────────────────────────────────────┐
│         Node.js Plugin Host（单进程，加载所有插件）    │
│                                                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐         │
│  │ 插件 A    │ │ 插件 B    │ │ 插件 C        │         │
│  │ (secretary)│ │ (review)  │ │ (notify)     │         │
│  └──────────┘ └──────────┘ └──────────────┘         │
│                       │                               │
│  ┌────────────────────▼───────────────────────────┐  │
│  │          Plugin SDK (npm 包)                     │  │
│  │     registerHook / llm:chat / feishu:send / ... │  │
│  └────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

**设计决策：单 Node 宿主，非多进程。**

| 决策 | 理由 |
|------|------|
| 一个 Node 进程加载所有插件（require） | 免 N 次冷启动、省内存 |
| 一个插件崩溃 → 整个宿主重启 | Supervisor 守护，自动恢复 |
| 3 次/60s 崩溃 → 熔断 | 禁用该插件 + TUI 告警 |
| 插件必须设计为无状态 | 状态通过 IPC 持久化到 Rust Core |

### 9.2 技术选型

| 层 | 技术 |
|---|---|
| 核心引擎 | Rust |
| TUI | Ratatui |
| 插件运行时 | Node.js 单进程宿主 + JSON-RPC IPC |
| 插件监管 | Rust Supervisor（心跳 ping/5s, 超时 2s, 3 次无响应 = 重启 + **宿主级熔断**: 5 次重启/5min → 逐步隔离最差插件） |
| 飞书集成 | 飞书 CLI (lark-cli) |
| Agent 配置 | YAML |
| 内部协议 | JSON-RPC 2.0 |

### 9.3 Hook 点位

```
Agent 生命周期:
  agent:before_create   → 创建前（可校验、改配置）
  agent:after_create    → 创建后（可自动注册 MCP、发通知）
  agent:before_start    → 启动前
  agent:after_start     → 启动后（可预热连接）
  agent:before_stop     → 停止前
  agent:before_delete   → 删除前（V2: 同步删除飞书 Bot）

消息流:
  message:received      → 收到飞书消息（可过滤、改写）
  message:before_send   → 发往飞书前（可格式化、加签名）
  message:routed        → Router 分派完成后（可审计）

任务流:
  task:created          → 新任务创建
  task:assigned         → 任务分派给 Agent
  task:blocked          → 任务阻塞（秘书插件可触发提醒）
  task:completed        → 任务完成

LLM 调用:
  llm:before_call       → LLM 调用前（可加 system prompt、上下文注入）
  llm:after_call        → LLM 返回后（可校验输出、记录 token 消耗）

System:
  system:startup        → Orchestrator 启动
  system:shutdown       → Orchestrator 关闭
  system:tick           → 定时脉冲（周期性任务，如秘书摘要）

Memory:
  memory:before_compress → 对话压缩前（可自定义摘要策略）
  memory:after_compress  → 压缩后（可写入外部存储、触发通知）
  memory:before_retrieve → 检索记忆前（可过滤、重排序）
  memory:after_retrieve  → 检索后（可注入额外上下文）
  memory:forget          → 记忆被遗忘前（可备份到外部）
```

### 9.4 Node.js 插件模型

```typescript
import { Plugin, registerPlugin } from '@xxx/sdk';

const plugin: Plugin = {
  name: 'secretary-agent',
  version: '1.0.0',

  async init(ctx) {
    await ctx.setupLLM('claude-sonnet-4');
    await ctx.connectMCP('feishu-base');
  },

  async onTick(ctx) {
    // system:tick — 每15分钟执行
    const tasks = await ctx.getTasks({ status: 'active' });
    const summary = await ctx.llm.chat(
      `总结以下任务进展: ${JSON.stringify(tasks)}`
    );
    await ctx.sendToFeishu({ content: `📋 ${summary}` });
  },
};

registerPlugin(plugin);
```

### 9.5 IPC 协议

```
协议格式: JSON-RPC 2.0 over stdio

Rust → Node.js (Hook 触发):
  { "jsonrpc": "2.0", "method": "hook:message:received", "params": { ... }, "id": 1 }

Node.js → Rust (调用系统能力):
  { "jsonrpc": "2.0", "method": "feishu:send", "params": { ... }, "id": 2 }

Rust → Node.js (响应):
  { "jsonrpc": "2.0", "result": "ok", "id": 2 }
```

### 9.6 插件目录结构

```
~/.my-company/
├── agents/                 # Agent 配置 (YAML)
│   ├── pm.yaml
│   └── dev.yaml
├── plugins/                # Node.js 插件
│   ├── secretary/
│   │   ├── plugin.json     # 清单: { name, version, hooks }
│   │   ├── index.ts        # 入口
│   │   └── node_modules/
│   └── auto-review/
│       ├── plugin.json
│       └── index.ts
├── mcp-configs/            # MCP 连接配置
│   └── github.json
└── logs/                   # 运行日志
```

### 9.7 内置 vs 用户插件

| 内置插件 (Rust, 随二进制, 有 Core 库级直接访问权限) | 用户插件 (Node.js, 自由扩展, 通过 IPC 调用 Core) |
|---|---|
| `secretary` — 秘书 Agent（Rust Core 模块，非插件。直接调用 LLM Gateway / Memory / Feishu Bridge，无 IPC 开销）| 代码审查 — 自动 review 工作流 |
| `agent:router` — 消息路由 + 意图识别 | 通知推送 — 自定义通知渠道 |
| `agent:registry` — Agent 注册中心 | 任务看板 — 可视化任务状态 |
| `feishu:bridge` — 飞书 CLI 集成 | 任何自定义插件... |

### 9.8 插件演进

| | V1 | V2 |
|---|---|---|
| 插件载体 | Node.js 进程（源码或 .js） | 支持 WASM 动态加载 |
| 分发方式 | 本地安装 / npm | 飞书群直接安装 .wasm 文件 |
| 安全性 | 进程隔离 | WASM 沙箱 + 权限声明 |

---

## 10. 后续规划（非 V1 范围）

- **飞书群创建 Agent**: 在群里 @Bot 即可创建新 Agent
- **独立 Bot 身份**: 每个 Agent 拥有独立飞书 Bot 身份，可被"拉入群"或"退出群"
- **Web 管理端**: 脱离终端，浏览器管理整个虚拟公司
- **云端运行时**: Agent 7×24 在线，用户离线也能持续工作
- **插件市场**: 社区贡献和分享 Node.js 插件
- **多群组支持**: 一个 Orchestrator 管理多个飞书群，不同群不同团队

---

## 11. 术语表

| 术语 | 定义 |
|------|------|
| Core | Rust 独立库，系统核心引擎。无 UI 依赖，TUI/CLI/Web 均为其消费者 |
| 秘书 Agent | 主 Agent，用户与虚拟公司的唯一对话桥梁。忙时主动推进，闲时静默归档 |
| Agent | 用户自定义的 AI 数字员工，有角色、能力、工具，背后是 LLM |
| Registry | Agent 注册中心，记录所有 Agent 的身份和能力 |
| LLM Gateway | Rust 中央 LLM 调用层。统一管理密钥、限流、重试、成本追踪 |
| Feishu CLI Bridge | 连接飞书 CLI 的适配层，负责消息监听和命令执行 |
| Plugin Host | 单个 Node.js 进程，加载所有用户插件，通过 JSON-RPC IPC 与 Core 通信 |
| Plugin Supervisor | Rust 侧守护进程。健康检查、崩溃重启、熔断保护 |
| Hook | 系统关键节点的挂载点，插件可注册回调来扩展功能 |
| MCP | Model Context Protocol，Agent 的工具连接协议。分全局和 Agent 专属 |
| Skill | Agent 的能力包，飞书 CLI Skills 或自定义能力 |
| Working Memory | 工作记忆，注入 LLM 上下文的当前对话，对话结束即清空 |
| Short-term Memory | 短期记忆，1-30 天内的结构化对话摘要，支持语义检索 |
| Long-term Memory | 长期记忆，跨项目的持久化知识和经验，经二次提炼后向量化存储 |
| Forgetting | 自然遗忘机制，基于艾宾浩斯衰减模型，记忆按时间和重要性自然衰减 |
| Busy/Idle Mode | 秘书的忙闲感知模式。忙时主动推进，闲时静默汇总。由飞书日历+TUI手动覆盖 |
| Priority Queue | Agent 收件箱。用户直连 > 紧急 > 秘书分派 > Agent 间协作 |
| Message Send Queue | 飞书 Bridge 发送队列。应对 5 QPS 群聊限流，优先级排序 + 同话题合并 |
| Rate Limiting (5 QPS) | 飞书 API 同一群聊所有 Bot 共享每秒 5 次调用上限 |

---

> **文档状态**: 设计完成，待用户审阅确认后进入实现计划阶段
