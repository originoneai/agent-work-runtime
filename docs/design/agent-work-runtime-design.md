# Agent Work Runtime（AWR）完整产品与工程设计方案

> **工作名称：Agent Work Runtime（AWR）**
>
> 定位：面向 Coding Agent / AI Agent 的**长周期任务状态与上下文运行时**。
>
> 核心目标：让一个持续数天、数周甚至数月的复杂任务，在 Session 切换、Compact、模型切换、进程退出、设备重启后，Agent 仍能以极低 Token 成本准确恢复“为什么做、做到哪里、接下来做什么、哪些规则不能违反”，而不必反复读取整个项目文档和历史。
>
> 本文档按“Codex 拿到即可施工”的粒度编写。

---

## 0. 一句话定义

**Git versions code. AWR versions agent work.**

AWR 不负责替代 Git，不负责做通用知识库，也不负责做聊天长期记忆。

AWR 管理四类核心信息：

- **Intent**：目标、方案、规则、验收条件；
- **State**：当前任务、依赖、状态、阻塞、分支、下一步；
- **Memory**：决策、事件、证据、Session、Artifact；
- **Context**：针对“当前 Agent + 当前任务 + 当前分支 + 当前 Token Budget”动态编译出的最小充分上下文。

真正的产品核心不是 SQLite，而是：

> **Context Compiler：从完整项目状态中，确定性生成当前 Agent 完成当前任务所需的最小充分上下文。**

---

# 1. 背景与问题

长周期 Coding Agent 开发通常会逐步形成四类权威材料：

1. Goal：目标；
2. Plan：方案；
3. Rules：开发必要规则；
4. Ledger：唯一台账。

随着工程推进，Ledger 往往继续承载：

- WorkItem；
- dependency；
- milestone；
- status；
- acceptance criteria；
- tests；
- evidence；
- blocker；
- summary；
- handoff；
- delivery SHA；
- remote receipt；
- 历史任务；
- 旧计划映射；
- risk；
- decision；
- 场景验收。

这种方式在人类治理层面是合理的，但会形成严重的 Agent Runtime 问题：

```text
Agent 只想知道“下一步做什么”
        ↓
读取完整 Goal / Plan / Rules / Ledger
        ↓
历史、证据、测试日志、旧任务重复进入上下文
        ↓
上下文膨胀
        ↓
Compact
        ↓
新 Session 再次读取大文件
        ↓
重新膨胀
```

问题并不是“没有长期记忆”。

真正的问题是：

> **存储模型和检索模型被绑定在一起。**

项目可以保存非常多的信息，但 Agent 每次只应该读取当前任务真正需要的极小子集。

AWR 的根本原则：

> **Store everything. Retrieve almost nothing.**

---

# 2. 产品定位

## 2.1 AWR 是什么

AWR 是：

> **Long-Horizon Agent Work Runtime / Persistent Work State for AI Agents**

中文建议定义为：

> **Agent 长周期任务状态与上下文管理运行时**

逻辑位置：

```text
Codex / Kimi / Grok / Claude Code / OpenCode / Cursor
                        │
                        ▼
                 CLI / MCP / SDK
                        │
                        ▼
                Agent Work Runtime
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
   State Engine    Context Compiler   Source Index
        │               │               │
        └───────────────┼───────────────┘
                        ▼
                  SQLite + FTS5
                        │
                        ▼
             Authoritative Project Sources
            Goal / Plan / Rules / Ledger / Git
```

## 2.2 AWR 不是什么

### 不是数据库内核

SQLite 只是底层 Storage Engine。

不自行实现 B+Tree、MVCC、WAL、Raft、SQL Optimizer 或分布式事务。

### 不是 Jira / Linear

普通任务系统解决“人如何管理任务”。

AWR 重点解决：

- Agent 每轮应该加载什么；
- Compact 后如何恢复；
- Agent 如何安全推进长任务；
- 多 Session / 多 Agent 如何不丢工作状态；
- 状态如何和真实项目源码、证据保持可追溯。

### 不是通用 Agent Memory

AWR 不以“把聊天 embedding 后相似召回”为主。

优先使用：

- ID；
- Status；
- DAG；
- Revision；
- Branch；
- Scope；
- Rule applicability；
- Source fingerprint；
- Event Delta。

### 不是 Vector Database

V1 不要求 embedding。

### 不是知识 Wiki

AWR 可以引用知识，但不负责把整个项目变成百科。

---

# 3. 设计目标

## G1：最小充分上下文

```bash
awr context compile --work R2-RUN-004 --budget 4000
```

应该返回：

- 当前 Goal；
- 当前 WorkItem；
- Acceptance；
- Dependencies；
- Active Rules；
- Relevant Decisions；
- Recent Delta；
- Blockers；
- Next Action；
- 必要 Evidence；
- Source Revision。

而不是整个项目历史。

## G2：Compact 无损恢复

```text
旧 Session
    ↓
checkpoint
    ↓
SessionDigest + ContextDelta
    ↓
新 Session
    ↓
bootstrap
    ↓
继续工作
```

## G3：所有事实可追溯

任何进入 Context 的事实必须能够回答：

- 来自哪里；
- 哪个版本；
- 当前是否 Fresh；
- 是否推断；
- 是否历史；
- 是否已验证。

## G4：不建立第二本真相源

V1：

> **项目文件仍然是 Authority，AWR DB 是 Projection + Runtime State。**

## G5：确定性优先

核心路径优先：

```text
Graph Filter
Scope Filter
Rule Filter
Revision Delta
Event Filter
Evidence Filter
Token Budget
```

只有确定性裁剪后仍超预算，才允许可选 LLM Compression。

---

# 4. V1 非目标

第一版明确不做：

- Web UI；
- SaaS；
- Cloud Sync；
- Team RBAC；
- 分布式数据库；
- CRDT；
- 自研 Vector DB；
- 自研 Graph DB；
- 默认自动修改自由 Markdown；
- 自动完成 WorkItem；
- 自动把组件测试冒充真实验收；
- 默认上传用户源码；
- 常驻云端记忆服务。

---

# 5. 总体模型

```text
                  Project
                     │
        ┌────────────┼────────────┐
        ▼            ▼            ▼
      Intent        State       Memory
        │            │            │
      Goal        WorkItem       Event
      Plan         Branch       Decision
      Rule         Claim        Evidence
   Acceptance      Blocker      Session
                    Status      Artifact
        │            │            │
        └────────────┼────────────┘
                     ▼
              Context Compiler
                     │
                     ▼
               Coding Agent
```

---

# 6. 核心领域对象

## 6.1 Project

```yaml
Project:
  id: prj_xxx
  external_key: personal-ai-os
  name: Personal AI OS
  root: /repo
  authority_mode: source_first
  current_branch_id: br_main
  project_revision: 128
```

`project_revision` 是 AWR 内部单调递增 revision。

以下变化会推进 revision：

- Source 重索引；
- WorkItem Projection 变化；
- Rule 变化；
- Decision 变化；
- Runtime State 变化；
- Branch 状态变化。

Context Pack 必须绑定 project_revision。

## 6.2 Source

```yaml
Source:
  id: src_xxx
  domain: ledger
  role: primary
  locator: file:///repo/ledger/project-ledger.yaml
  format: yaml
  adapter: yaml-ledger-v1
  revision: 42
  fingerprint: sha256:...
  freshness: fresh
```

V1 支持：

```text
file://
git://
```

未来可扩展：

```text
https://
mcp://
connector://
```

## 6.3 Goal

```yaml
Goal:
  id: 01K...
  external_key: G03
  title: ...
  status: active
  priority: high
  success_criteria: [...]
  source_ref: ...
  source_revision: ...
```

## 6.4 Plan

```yaml
Plan:
  id:
  external_key:
  title:
  status:
  scope:
  summary:
  source_ref:
  source_revision:
```

## 6.5 Rule

```yaml
Rule:
  id:
  external_key:
  severity: hard | soft | info
  text:
  scope:
    type: project | path | tag | work_item | agent
    value: ...
  source_ref:
  source_revision:
```

**Hard Rule 不能因为 Token Budget 被裁掉。**

## 6.6 WorkItem

```yaml
WorkItem:
  id:
  external_key: R2-RUN-004
  title:
  kind:
  required:
  raw_status:
  status:
  priority:
  milestone:
  score:
  evidence_level:
  summary:
  next_action:
  blocker:
  acceptance:
  source_ref:
  source_revision:
```

推荐规范状态：

```text
planned
ready
claimed
in_progress
blocked
completed
cancelled
```

但必须同时保留：

```text
raw_status
normalized_status
```

unknown 不猜测。

## 6.7 Edge

统一表达关系：

```text
WorkItem depends_on WorkItem
WorkItem satisfies Goal
WorkItem governed_by Rule
WorkItem supported_by Evidence
Decision affects WorkItem
Plan implements Goal
```

V1 使用 SQLite 普通表 + recursive CTE，不引入 Graph DB。

## 6.8 Decision

```yaml
Decision:
  id:
  external_key: ADR-112
  status: proposed | accepted | superseded | rejected
  title:
  decision:
  rationale:
  source_ref:
  source_revision:
```

Context 默认只加载：

- accepted；
- 未 superseded；
- 与当前 Goal / WorkItem / Path 相关。

## 6.9 Evidence

```yaml
Evidence:
  id:
  work_item_id:
  type:
  level:
  summary:
  locator:
  sha256:
  source_revision:
  verified_at:
```

建议 Evidence Level：

```text
designed
implemented
locally_verified
real_environment_validated
release_candidate
released
```

Context 默认只返回证据摘要和引用，不返回完整正文。

## 6.10 Event

Event Store 必须 append-only。

事件示例：

```text
work_claimed
work_started
test_started
test_failed
patch_applied
test_passed
source_changed
app_installed
native_validation_passed
blocked
unblocked
decision_added
evidence_recorded
checkpoint_created
session_ended
```

原则：

> **WorkItem 保存当前状态，Event 保存发生过什么。**

不要把过程历史无限追加回 WorkItem.tests 或 summary。

## 6.11 Session

```yaml
Session:
  id:
  project_id:
  work_item_id:
  branch_id:
  agent_id:
  provider:
  model:
  status:
  started_at:
  ended_at:
  start_project_revision:
  end_project_revision:
  last_checkpoint_id:
```

同一 WorkItem 可以在 Codex、Kimi、Grok 等多个 Session 间连续推进。

## 6.12 Checkpoint

Compact / Session End 前写：

```yaml
Checkpoint:
  id:
  session_id:
  project_revision:
  context_hash:
  digest:
  next_action:
  open_loops:
  changed_entities:
  created_at:
```

## 6.13 Branch

AWR Branch 是 **Agent Work Branch**，不是 Git 替代品。

```yaml
Branch:
  id:
  name:
  parent_branch_id:
  git_ref:
  fork_project_revision:
  status: active | merged | abandoned
```

Session、Event、Claim、Context、Evidence 都可绑定 Branch。

## 6.14 Artifact

大型正文不直接放 SQLite：

- stdout；
- test log；
- build report；
- diff；
- screenshot；
- bundle；
- trace。

SQLite 只存 metadata：

```yaml
Artifact:
  id:
  type:
  locator:
  sha256:
  size:
  mime:
  source_event_id:
```

## 6.15 MutationProposal

修改权威 Source 必须经过 Proposal：

```yaml
MutationProposal:
  id:
  project_id:
  work_item_id:
  source_id:
  base_fingerprint:
  mutation_type:
  patch:
  status:
  created_by_session:
```

状态：

```text
draft
ready
approved
applied
conflict
rejected
failed
```

---

# 7. Authority Model

## 7.1 Source-First

V1：

```text
Goal / Plan / Rules / Ledger / Git
                │
                ▼
              Indexer
                │
                ▼
        AWR Projection DB
```

权威关系：

```text
Source > Projection
```

如果 Source 和 DB 冲突：

```text
Source 胜出
Projection 标 stale
```

## 7.2 Runtime State 由 DB 权威

以下对象允许 DB 权威：

- Session；
- Event；
- Checkpoint；
- Claim；
- Context Pack；
- Runtime Branch State；
- Artifact Metadata。

原因：它们本来就是 AWR 自己产生的运行数据。

## 7.3 后续可扩展

未来可支持：

```text
source_first
managed_source
db_first
```

但 V1 只正式支持：

```text
source_first
```

---

# 8. Source Manifest

项目使用极小配置：

```toml
# .awr/project.toml

[project]
name = "Personal AI OS"

[[sources]]
domain = "goal"
role = "primary"
path = "docs/GOALS.md"
adapter = "markdown-heading-v1"

[[sources]]
domain = "plan"
role = "primary"
path = "docs/product-plan-v2/README.md"
adapter = "markdown-heading-v1"

[[sources]]
domain = "rules"
role = "primary"
path = "ledger/LEDGER_RULES.md"
adapter = "markdown-rules-v1"

[[sources]]
domain = "ledger"
role = "primary"
path = "ledger/project-ledger.yaml"
adapter = "yaml-ledger-v1"

[[sources]]
domain = "decisions"
role = "supporting"
path = "docs/adr"
adapter = "markdown-directory-v1"
```

`.awr/project.toml` 只保存 locator 和解析配置，不复制业务正文。

---

# 9. Source Adapter

统一接口：

```rust
trait SourceAdapter {
    fn discover(&self, ...);
    fn fingerprint(&self, ...);
    fn parse(&self, ...);
    fn project(&self, ...);
    fn plan_mutation(&self, ...);
    fn apply_mutation(&self, ...);
}
```

V1 必须实现：

### yaml-ledger-v1

支持：

- work_items；
- milestones；
- goals；
- status；
- acceptance；
- dependency；
- evidence refs。

允许严格 fingerprint-bound deterministic mutation。

### markdown-heading-v1

支持 heading / section range / section fingerprint。

默认 read-only。

### markdown-rules-v1

提取 Rule。

### markdown-directory-v1

读取 ADR / RFC 等目录。

---

# 10. Storage Engine

推荐：

```text
SQLite
WAL
Foreign Keys
FTS5
JSON1
```

启动：

```sql
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;
PRAGMA busy_timeout=5000;
```

V1 不做数据库内核抽象过度设计。

---

# 11. 逻辑 Schema

以下为实现必须覆盖的主表：

```text
projects
sources
goals
plans
rules
work_items
edges
decisions
evidence
branches
sessions
claims
events
checkpoints
artifacts
mutation_proposals
context_packs
schema_migrations
```

关键字段规则：

- 内部 ID：ULID；
- 外部业务 ID：`external_key`；
- 结构化对象必须有 `revision`；
- Source Projection 必须有 `source_id/source_ref/source_revision`；
- 可并发 mutation 必须使用 `expected_revision`；
- Source mutation 必须使用 `base_fingerprint`。

典型 WorkItem 表：

```sql
CREATE TABLE work_items (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  external_key TEXT NOT NULL,
  title TEXT NOT NULL,
  kind TEXT,
  required INTEGER NOT NULL DEFAULT 0,
  raw_status TEXT,
  status TEXT NOT NULL,
  priority TEXT,
  milestone TEXT,
  score INTEGER,
  evidence_level TEXT,
  summary TEXT,
  next_action TEXT,
  blocker TEXT,
  acceptance_json TEXT NOT NULL DEFAULT '[]',
  source_id TEXT NOT NULL,
  source_ref_json TEXT NOT NULL,
  source_revision INTEGER NOT NULL,
  revision INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(project_id, external_key)
);
```

Event：

```sql
CREATE TABLE events (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  work_item_id TEXT,
  session_id TEXT,
  branch_id TEXT,
  event_type TEXT NOT NULL,
  importance TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
```

Event append-only，禁止修改既有正文。

Claim 单独建表，不复用 Source Ledger 里的 owner：

```sql
CREATE TABLE claims (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  work_item_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  branch_id TEXT,
  status TEXT NOT NULL,
  acquired_at INTEGER NOT NULL,
  expires_at INTEGER,
  released_at INTEGER
);
```

这样明确区分：

```text
source owner
execution agent
runtime claim
```

---

# 12. FTS5

V1 搜索只做 FTS5。

索引：

- Goal；
- Plan；
- Rule；
- WorkItem；
- Decision；
- Event summary；
- Evidence summary。

不要索引：

- 完整 stdout；
- 大日志；
- 二进制；
- 未脱敏 Source Body。

Embedding 只保留接口，不做必选依赖。

---

# 13. Ready Work

定义：

```text
planned
+
所有 required dependency 均 completed
+
没有 active blocker
+
没有冲突 runtime claim
=
ready
```

使用 SQLite recursive CTE 求 dependency closure。

---

# 14. Mutation Protocol

Agent 禁止直接 SQL UPDATE。

只开放领域动作：

```text
claim
release
progress
block
unblock
complete
cancel
handoff
decision
evidence
checkpoint
```

## complete()

必须按以下流程：

```text
检查当前 revision
↓
检查 required dependencies
↓
检查 acceptance criteria
↓
检查 required evidence
↓
检查 blocker
↓
生成 Source Mutation Proposal
↓
fingerprint recheck
↓
批准/应用
↓
re-index Source
↓
确认 Projection 真正变为 completed
↓
记录 Event
```

如果 Adapter 不支持安全写：

```text
proposal_required
```

不能只改 DB 冒充 Source 已完成。

---

# 15. Source Mutation Safety

任何 Source Mutation 必须绑定：

```text
source_id
base_fingerprint
exact target
expected revision
mutation intent
patch
```

应用前：

```text
re-read
↓
re-fingerprint
↓
compare
```

不一致：

```text
SourceConflict
```

不自动猜测和覆盖。

---

# 16. Context Compiler

输入：

```yaml
ContextRequest:
  project_id:
  work_item_id:
  branch_id:
  session_id:
  agent_id:
  token_budget:
  intent:
```

输出：

```yaml
ContextPack:
  project_revision:
  source_revisions:
  hard_context:
  task_context:
  recent_delta:
  relevant_memory:
  omitted_refs:
  completeness:
  token_estimate:
  context_hash:
```

编译流程：

1. Freshness Check；
2. Resolve Current Work；
3. Dependency Closure；
4. Rule Applicability；
5. Decision Selection；
6. Recent Delta；
7. Evidence Selection；
8. Token Budget；
9. Deterministic Render；
10. Optional LLM Compression。

---

# 17. Freshness

每次 Context Compile 前检查关键 Source fingerprint。

如果变化：

```text
incremental re-index
```

如果无法读取：

```text
stale / unavailable
```

禁止把旧 Projection 标成 current。

---

# 18. Context 四级模型

## L0 Bootstrap

Session Start / Compact 后自动读取：

```text
Project
Current Phase
Current WorkItem
Status
Next Action
Blocker
Critical Rules
Last Checkpoint
Revision
```

目标：

```text
<= 1000 tokens
```

## L1 Work Context

真正开始任务前读取：

```text
Goal
WorkItem
Acceptance
Dependencies
Rules
Decisions
Recent Delta
Evidence Gaps
Branch
Source Revisions
```

目标：

```text
<= 5000 tokens
```

## L2 Drill Down

显式查询：

```bash
awr work history R2-RUN-004
awr decision show ADR-112
awr evidence show EV-123
awr source show ledger
```

## L3 Raw Artifact

显式：

```bash
awr artifact cat ART-xxx
```

默认绝不进入 Context。

---

# 19. Token Budget

4000 Token 的默认分配建议：

```text
Hard Rules                    12%
Current Goal                  10%
Current WorkItem              20%
Acceptance Criteria           15%
Dependencies / Blockers       10%
Relevant Decisions             8%
Recent Delta                  15%
Evidence Summary               5%
Metadata / References          5%
```

不可被 LLM Summary 改写的 Hard Context：

- Hard Rule；
- Acceptance；
- ID；
- Status；
- Revision；
- Fingerprint；
- Blocker；
- Next Action。

---

# 20. Deterministic Compression

优先采用：

### 结构裁剪

不相关实体根本不查询。

### 字段裁剪

WorkItem 默认只返回：

```text
id
title
status
acceptance
summary
next_action
blocker
```

### Event 聚合

默认只取：

```text
Checkpoint 之后 Delta
+
最近 high/critical events
```

### 历史折叠

旧过程只给摘要，原事件保持可 drill-down。

---

# 21. Optional LLM Compression

只有 deterministic context 仍超预算时才调用。

例如：

```text
15 KB deterministic result
↓
optional LLM compression
↓
5 KB
```

LLM 仅允许压缩：

- 旧 Event history；
- rationale；
- discussion。

核心状态禁止改写。

---

# 22. Context Hash

每个 Pack：

```text
context_hash = SHA256(
  project_revision
  + work_item_revision
  + branch
  + source_revisions
  + selected_entity_ids
  + rendered_context
)
```

同一状态、同一请求应得到稳定 hash。

---

# 23. Compact 协议

Compact 前：

```bash
awr session checkpoint \
  --next "rebuild and native retest" \
  --open-loop "summary timeout remains"
```

AWR：

```text
读取 Session Delta
↓
生成 Checkpoint
↓
写 SessionDigest
↓
记录 changed entities
↓
保存 Next Action
↓
记录 Event
```

Compact 后：

```bash
awr context bootstrap
```

直接恢复：

```text
CURRENT WORK
LAST CHECKPOINT
WHAT CHANGED
NEXT ACTION
CRITICAL RULES
```

Compact 被定义为：

> **Checkpoint + Runtime Restart**

而不是重新理解整个项目。

---

# 24. Session Resume

```bash
awr session resume
```

流程：

```text
查最后 active/incomplete Session
↓
校验 project revision
↓
校验 source freshness
↓
生成 ContextDelta
↓
继续
```

默认不重新读取完整历史。

---

# 25. 多 Agent

V1 只解决：

- Claim；
- Session；
- Branch；
- Optimistic Revision；
- Source Fingerprint。

不实现分布式锁。

Claim 可有 TTL，过期可回收。

任何写入使用：

```text
expected_revision
```

任何 Source 写使用：

```text
expected_fingerprint
```

---

# 26. Branch

Git 管 Source Branch。

AWR 管 Work Branch。

```text
main
├── br_run004_fix
├── br_kimi
└── br_experiment
```

AWR Branch 保存：

- fork revision；
- git ref；
- sessions；
- events；
- claims；
- contexts；
- evidence。

`awr branch context <name>` 返回：

```text
main 当前有效状态
+
fork revision 之后本 Branch Runtime Delta
```

V1 Merge：

```text
Git / Source Merge
↓
AWR Re-index
↓
Branch Merge Summary
↓
Close Branch
```

不实现源码 CRDT 或三方 merge 引擎。

---

# 27. Search

优先结构化搜索：

```bash
awr search --type event --work R2-RUN-004 --status failed
```

FTS：

```bash
awr search "Kimi 403"
```

返回：

```text
entity id
type
summary
source ref
rank
```

V2 才考虑 embedding。

---

# 28. CLI

V1 主入口是 CLI：

```bash
awr init

awr source list
awr source scan
awr source reindex

awr status
awr ready

awr work show <id>
awr work claim <id>
awr work progress <id>
awr work block <id>
awr work complete <id>
awr work handoff <id>
awr work history <id>

awr context bootstrap
awr context compile --work <id> --budget 4000
awr context delta

awr session start
awr session checkpoint
awr session resume
awr session end

awr decision show <id>
awr evidence add ...
awr evidence show <id>

awr branch list
awr branch create <name>
awr branch switch <name>
awr branch context <name>
awr branch close <name>

awr search "..."
awr doctor
```

CLI 默认输出必须短。

`awr status` 示例：

```text
Project: Personal AI OS
Phase: R2-M1
Current: R2-RUN-004
Status: in_progress
Blocker: none
Next: rebuild + native retest + push
Ready: 3
Revision: 128
```

---

# 29. MCP

MCP Tool Schema 本身也吃上下文。

V1 只做 8 个：

```text
awr_project_status
awr_work_ready
awr_work_get
awr_context_compile
awr_work_transition
awr_event_append
awr_evidence_record
awr_search
```

不要把每一个 CLI 子命令都变成 MCP Tool。

---

# 30. Agent 使用协议

项目只需给 Coding Agent 一段短规则：

```text
This project uses AWR.

At session start:
1. Run `awr context bootstrap`.

Before working on a task:
2. Run `awr context compile --work <id>`.

Do not read the full project ledger unless AWR reports missing/stale context.

Before compact/session end:
3. Run `awr session checkpoint`.

All task transitions must go through AWR.
Never directly mark a task completed without required evidence.
```

目标：

> AGENTS.md 不再承载整个项目状态。

---

# 31. 项目初始化

```bash
awr init
```

流程：

```text
发现 Project Root
↓
创建 .awr/project.toml
↓
创建 .awr/state.db
↓
发现候选 Source
↓
用户确认 authority mapping
↓
首次 index
↓
生成 project revision
```

推荐布局：

```text
project/
├── .awr/
│   ├── project.toml
│   ├── state.db
│   ├── artifacts/
│   └── cache/
├── docs/
├── ledger/
└── ...
```

`.awr/state.db` 默认加入 `.gitignore`。

团队真正共享的是 Git + Source，不是 SQLite 文件。

---

# 32. 安全边界

必须实现：

## 路径 containment

Source 必须位于 Project Root 或显式授权 Root。

拒绝：

```text
../
symlink escape
```

## 文件大小限制

Adapter 必须有 read cap，例如：

```text
Markdown 2 MiB
YAML 4 MiB
```

## Secret

禁止默认存：

- API Key；
- OAuth Token；
- Password；
- 完整 Private Prompt；
- 未脱敏环境变量。

## Event Payload

必须有 schema、字段白名单和 size cap。

---

# 33. 一致性

Source Projection 必须绑定：

```text
source_id
source_revision
source_fingerprint
source_ref
```

Runtime Mutation：

```text
BEGIN IMMEDIATE
read revision
compare expected_revision
write
append event
project_revision++
COMMIT
```

---

# 34. Crash Recovery / Doctor

SQLite WAL 负责基础恢复。

AWR 自己负责 reconciliation：

- active session；
- expired claim；
- pending mutation；
- incomplete checkpoint；
- orphan artifact；
- interrupted branch。

```bash
awr doctor
```

检查：

```text
SQLite integrity
schema version
source access
source fingerprint
stale projection
dangling edge
missing dependency
expired claim
orphan session
orphan artifact
invalid branch
failed mutation
```

---

# 35. 现有四文件接入

已有：

```text
Goal
Plan
Rules
Ledger
```

第一阶段不迁移正文：

```text
四文件
  ↓
Indexer
  ↓
AWR Projection
```

Agent 默认查 AWR。

用户仍然编辑原文件。

Source 变化：

```text
invalidate
↓
incremental re-index
↓
project_revision++
```

这保证 AWR 不是第二本 Ledger。

---

# 36. 大型 Ledger 的处理方式

大型 Ledger 不需要人工拆文件。

Indexer 第一次完整读取并投影成：

```text
project_state
active_plan
milestones
work_items
historical_work_items
risks
decisions
evidence_log
```

以后：

```bash
awr work show R2-RUN-004
```

只读一个任务 Projection。

历史只有显式请求才进入 Context。

---

# 37. Event 化原则

原本 WorkItem 可能不断累积：

```yaml
tests:
  - 第一次失败
  - 第二次修复
  - 第三次重跑
  - 第四次安装
  - 第五次复测
```

AWR 后：

WorkItem：

```yaml
summary: 当前修复已通过本地回归
next_action: 原位安装后真实复测
```

过程进入 Event：

```text
test_failed
patch_applied
regression_passed
build_succeeded
app_installed
native_validation_passed
```

Current State 永远保持小。

---

# 38. Event 与 Evidence 分离

Event：

> 发生了什么。

Evidence：

> 哪个事实已经被验证。

例如：

```text
event:
  test_passed
```

不等于：

```text
evidence:
  locally_verified
```

Evidence 还需要绑定 command、source SHA、report、scope 等。

---

# 39. Context Completeness

ContextCompiler 除了文本，还必须返回机器字段：

```yaml
completeness:
  source_fresh: true
  work_item_found: true
  acceptance_complete: true
  rules_complete: true
  dependencies_complete: true
  decision_context_complete: true
  evidence_gaps:
    - real_environment_validation
```

如果关键字段不完整，Context 必须显式标：

```text
CONTEXT INCOMPLETE
```

---

# 40. Context 默认输出

```markdown
# AWR Context

Project: Personal AI OS
Revision: 128
Branch: main

## Current Work
R2-RUN-004 — ...

Status: in_progress

## Goal
G03 ...

## Next Action
...

## Acceptance
1. ...
2. ...
3. ...

## Dependencies
- RUN-002: completed
- RUN-003: in_progress

## Hard Rules
- ...
- ...

## Recent Delta
- ...
- ...

## Relevant Decisions
- ADR-112 ...

## Evidence Gaps
- real environment lifecycle incomplete

## Source Refs
...
```

---

# 41. 性能目标

本地单项目目标规模：

```text
WorkItems < 10k
Project Objects < 100k
Events < 1M
```

目标延迟：

```text
status < 20 ms
work show < 20 ms
ready < 50 ms
context compile < 100 ms
FTS search < 100 ms
普通增量 reindex < 500 ms
```

性能核心指标并不是 QPS，而是上下文效率。

---

# 42. Token Benchmark

核心指标：

```text
Compression Ratio
=
Full Source Estimated Tokens
/
Compiled Context Tokens
```

目标：

```text
> 20x
```

大型 Ledger 争取：

```text
50x ~ 100x
```

同时 Hard Fact Recall 必须 100%：

- WorkItem ID；
- Status；
- Next Action；
- Acceptance；
- Blocker；
- Hard Rules；
- unresolved required dependencies。

---

# 43. Compact Benchmark

模拟：

```text
Session A
→ events
→ source change
→ checkpoint
→ compact
→ Session B
→ bootstrap
```

验证：

```text
Current WorkItem 一致
Next Action 一致
Open Loops 一致
Hard Rules 一致
Source Delta 被包含
旧无关历史未进入 Context
```

---

# 44. 技术栈

推荐 Rust。

理由：

- 单二进制；
- 本地优先；
- 跨平台；
- SQLite 嵌入；
- CLI 友好；
- 后续可直接承载 MCP Server。

建议依赖：

```text
rusqlite
clap
serde
serde_json
toml
sha2
ulid
anyhow
thiserror
walkdir
notify
```

V1 建议：

```text
rusqlite + 明确 SQL
```

不要先上复杂 ORM。

---

# 45. Repo 结构

```text
agent-work-runtime/
├── Cargo.toml
├── crates/
│   ├── awr-core/
│   ├── awr-store/
│   ├── awr-source/
│   ├── awr-context/
│   ├── awr-runtime/
│   ├── awr-cli/
│   └── awr-mcp/
├── adapters/
│   ├── yaml-ledger/
│   ├── markdown-heading/
│   └── markdown-rules/
├── tests/
│   ├── fixtures/
│   ├── long-ledger/
│   └── compact-recovery/
├── docs/
└── examples/
```

模块职责：

### awr-core

领域对象与状态机。

### awr-store

SQLite、Migration、Query、FTS、Integrity。

### awr-source

Source Manifest、Fingerprint、Adapter、Index、Mutation Planning。

### awr-context

Dependency、Rule Applicability、Delta、Token Budget、Context Pack。

### awr-runtime

Session、Claim、Event、Checkpoint、Branch。

### awr-cli

CLI。

### awr-mcp

极简 MCP。

---

# 46. Error Model

统一 typed error：

```text
NotFound
SourceUnavailable
SourceStale
SourceConflict
RevisionConflict
DependencyBlocked
ClaimConflict
RuleViolation
EvidenceMissing
MutationUnsupported
MutationConflict
ContextIncomplete
BudgetExceeded
InvalidTransition
```

不要把核心错误全部变成 string。

---

# 47. WorkItem 状态机

```text
planned
  ↓
ready
  ↓
claimed
  ↓
in_progress
  ├── blocked
  │      ↓
  │   in_progress
  ↓
completed
```

另外：

```text
planned/in_progress/blocked → cancelled
```

completed 默认不可回退。

需要 reopen：

```text
reason
+
event
+
revision
```

---

# 48. 开发阶段

## Phase 0：Skeleton

交付：

- Cargo Workspace；
- Core Types；
- SQLite；
- Migration；
- CLI Shell；
- Doctor。

## Phase 1：Source Projection

交付：

- Source Manifest；
- Fingerprint；
- YAML Ledger Adapter；
- Markdown Adapter；
- Project Revision；
- Incremental Reindex。

## Phase 2：Task Graph

交付：

- Goal；
- Plan；
- Rule；
- WorkItem；
- Edge；
- Ready；
- Decision；
- Evidence。

## Phase 3：Runtime State

交付：

- Session；
- Claim；
- Event；
- Checkpoint；
- Artifact Ref。

## Phase 4：Context Compiler

交付：

- Bootstrap；
- Work Context；
- Dependency Closure；
- Rule Applicability；
- Recent Delta；
- Token Budget；
- Context Hash。

这是第一个真正体现产品价值的 Phase。

## Phase 5：Compact Recovery

交付：

- Session Checkpoint；
- Resume；
- ContextDelta；
- Open Loops；
- Next Action。

## Phase 6：Source Mutation

交付：

- Mutation Proposal；
- Fingerprint Guard；
- YAML Safe Update；
- Apply 后 Reindex。

Markdown 默认 read-only。

## Phase 7：Branch

交付：

- Branch Create；
- Branch Context；
- Branch Delta；
- Branch Close；
- Git Ref Binding。

## Phase 8：MCP + Agent Adapter

交付：

- 8 个 MCP Tools；
- Codex Integration；
- Kimi / Grok Usage Guide。

## Phase 9：Benchmark + Open Source

交付：

- Long Ledger Benchmark；
- Compact Benchmark；
- Token Benchmark；
- README；
- Examples；
- Release Binary。

---

# 49. 推荐第一批 Work Items

```text
AWR-P0-001  Cargo Workspace、CLI、Core Types
AWR-P0-002  SQLite Store、Migration、WAL、Integrity
AWR-P0-003  Project / Source Schema

AWR-P1-001  Source Manifest
AWR-P1-002  Fingerprint / Freshness
AWR-P1-003  YAML Ledger Adapter
AWR-P1-004  Markdown Heading / Rules Adapter

AWR-P2-001  Goal / Plan / Rule Projection
AWR-P2-002  WorkItem / Edge / Ready Query
AWR-P2-003  Decision / Evidence

AWR-P3-001  Session / Claim
AWR-P3-002  Append-only Event Store
AWR-P3-003  Checkpoint / Artifact Ref

AWR-P4-001  Context Bootstrap
AWR-P4-002  Work Context Compiler
AWR-P4-003  Rule Applicability
AWR-P4-004  Dependency Closure
AWR-P4-005  Recent ContextDelta
AWR-P4-006  Token Budget / Context Hash

AWR-P5-001  Session Checkpoint
AWR-P5-002  Compact Resume

AWR-P6-001  Mutation Proposal
AWR-P6-002  YAML Fingerprint-bound Apply

AWR-P7-001  Work Branch
AWR-P7-002  Branch Context Delta

AWR-P8-001  Minimal MCP
AWR-P8-002  Codex Integration

AWR-P9-001  Long Ledger Benchmark
AWR-P9-002  Compact Recovery Benchmark
AWR-P9-003  Open Source Docs / Release
```

---

# 50. V0.1 Done Definition

V0.1 可以发布，当且仅当：

## Storage

- SQLite/WAL；
- migration；
- integrity；
- crash reopen。

## Source

- YAML Ledger；
- Markdown Section；
- fingerprint；
- stale detection；
- read-only import。

## Work

- Goal；
- Plan；
- Rules；
- WorkItem；
- DAG；
- Ready；
- Decision；
- Evidence。

## Runtime

- Session；
- Claim；
- Event；
- Checkpoint。

## Context

- Bootstrap；
- Work Context；
- Token Budget；
- Context Hash；
- Delta。

## Compact

- checkpoint；
- resume；
- open loop；
- next action。

## Safety

- source-first；
- no silent write；
- mutation fingerprint guard；
- hard rule never omitted。

---

# 51. 第一版绝对不要做

```text
Vector DB
Graph DB
Web UI
Server
Cloud Account
Team
RBAC
Mobile
Distributed Lock
CRDT
LLM Mandatory Path
```

---

# 52. 真实验收 Fixture

必须拿一个真实复杂项目验证：

```text
30+ 当前 WorkItems
100+ 历史 WorkItems
多个 Milestone
多个 Decision
大量 Evidence
大量 tests/history
数百 KB Ledger
```

验证：

```bash
awr init
awr source scan
awr status
awr work show <current>
awr context bootstrap
awr context compile --work <current> --budget 4000
awr session checkpoint
awr session resume
```

成功标准：

1. Agent 不读取完整 Ledger；
2. Bootstrap <= 1000 tokens；
3. Work Context <= 5000 tokens；
4. 当前 WorkItem、Acceptance、Hard Rules、Dependency、Blocker、Next Action 100% 准确；
5. 历史 WorkItem 默认不进入 Context；
6. Source 修改后旧 Projection 自动 stale；
7. Compact 后恢复正确；
8. 删除 DB 后可以从 Source 重建核心 Projection；
9. Runtime Event/Checkpoint 独立于 Source；
10. 无任何未经授权的项目文件改写。

---

# 53. 开源定位

README 首页可以直接写：

> **AWR is persistent work state for long-running AI agents.**
>
> Your coding agent should not reread a 300 KB project ledger after every compact.
>
> AWR indexes project goals, rules, plans and task graphs, tracks sessions and evidence, and compiles only the context needed for the work happening now.

传播句：

> **Git remembers the code. AWR remembers the work.**

或者：

> **Stop reloading the project. Resume the work.**

License 推荐：

```text
Apache License 2.0
```

开源 Core 聚焦：

```text
local-first
single-user
SQLite
CLI
MCP
Context Compiler
```

未来商业能力另做：

```text
Team shared state
Cloud Sync
Fleet
RBAC
Policy
Audit
Enterprise Source Adapter
Hosted Context Service
Multi-device
```

---

# 54. 十条产品铁律

## P1

**Source 是真相，DB 是投影。**

## P2

**Current State 和 Historical Event 分离。**

## P3

**Context 是 Query Result，不是 File。**

## P4

**Hard Context 不依赖 LLM Summary。**

## P5

**Agent 只通过领域动作推进状态。**

## P6

**Compact = Checkpoint + Restart。**

## P7

**Git 管源码 Branch，AWR 管 Work Branch。**

## P8

**所有 Source Mutation 都必须 fingerprint-bound。**

## P9

**未知状态不猜、冲突不猜、缺证据不猜。**

## P10

**默认只读取最小充分上下文。**

---

# 55. Codex 实施约束

Codex 开发本项目时必须遵循：

1. 不扩大 V0.1 范围；
2. 不实现数据库内核；
3. 不加入 Web UI；
4. 不引入 Vector DB；
5. 不引入 Graph DB；
6. 不将 LLM 放入必经核心路径；
7. SQLite 是唯一 Runtime Storage Engine；
8. 所有外部 Mutation 必须经过 Domain Service；
9. Source-First 下 DB 不可冒充 Source Truth；
10. Context Pack 必须绑定 revision 和 hash；
11. Event append-only；
12. Artifact 大正文不进入 SQLite；
13. 所有路径做 containment check；
14. 所有文件 Mutation 做 fingerprint recheck；
15. Hard Rule 100% 进入适用 Context；
16. 每 Phase 必须有定向测试和完整 regression；
17. 先做 deterministic contract，再做 CLI/MCP；
18. 不以 component pass 冒充 E2E pass；
19. 不提前引入分布式组件；
20. 代码优先简单、可测试、可替换。

---

# 56. 最终架构

```text
                        ┌────────────────────┐
                        │   Coding Agent     │
                        └─────────┬──────────┘
                                  │
                         CLI / MCP / SDK
                                  │
                                  ▼
                    ┌─────────────────────────┐
                    │    Agent Work Runtime   │
                    │                         │
                    │  ┌───────────────────┐  │
                    │  │ Context Compiler  │  │
                    │  └─────────┬─────────┘  │
                    │            │            │
                    │  ┌─────────▼─────────┐  │
                    │  │ State Query Engine│  │
                    │  └─────────┬─────────┘  │
                    │            │            │
                    │  ┌─────────▼─────────┐  │
                    │  │ Runtime Engine    │  │
                    │  │Session/Event/etc. │  │
                    │  └─────────┬─────────┘  │
                    │            │            │
                    │  ┌─────────▼─────────┐  │
                    │  │ Source Projection │  │
                    │  └─────────┬─────────┘  │
                    └────────────┼────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ SQLite / WAL / FTS5     │
                    └────────────┬────────────┘
                                 │
             ┌───────────────────┼────────────────────┐
             ▼                   ▼                    ▼
         Goal/Plan             Rules               Ledger
                                                       │
                                                       ▼
                                                      Git
```

---

# 57. 最终成功标准

AWR 的价值不是“存更多”。

恰恰相反：

> **项目可以越来越大，但每次给 Agent 的上下文仍然非常小。**

理想状态：

```text
项目 1 天
20 KB Source
→ 3 KB Context

项目 1 个月
2 MB Source + History
→ 3 KB Context

项目 1 年
100 MB Artifact + Event
→ 3 KB Context
```

随着项目增长：

```text
Storage ↑
History ↑
Evidence ↑
Events ↑
Artifacts ↑
```

但：

```text
Current Context ≈ Constant
```

最终可以用一个公式衡量：

```text
Context Quality
=
Task Relevance
× State Correctness
× Rule Completeness
× Source Freshness
÷ Token Cost
```

第一版只把四件事情做到极致：

```text
1. Persist Work State
2. Track Work Events
3. Restore Long Tasks
4. Compile Minimal Context
```

SQLite 是底座。

真正的产品是：

> **Agent 长任务 Work Runtime + Context Compiler。**
