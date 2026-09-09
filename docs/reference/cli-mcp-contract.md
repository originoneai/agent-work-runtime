# CLI / MCP 输出与错误契约

适用于当前 `0.1.0-dev` 实现。V1 MCP 固定提供 8 个工具，调用现有领域服务。CLI 是完整的项目管理入口；MCP 只暴露下表中的动作。

## 动作映射

CLI 示例统一以 `awr --project /absolute/project --json` 为前缀。表中的 `R` 是最近一次观察到的 `project_revision`，会话由 `awr session start` 创建。

| MCP 工具 | CLI 对应动作 | 参数对应与返回值 |
| --- | --- | --- |
| `awr_project_status` | `status [--branch NAME_OR_ID]` | `branch` 对应 `--branch`；项目、分支、状态数量、当前/建议任务和下一步 |
| `awr_work_ready` | `ready [--limit N] [--branch NAME_OR_ID]` | `limit` 默认 10，范围 1..100；就绪项、诊断、认领与截断提示 |
| `awr_work_get` | `work show WORK [--branch NAME_OR_ID] [--source-sha SHA]` | `work`、`branch`、`source_sha`；任务、验收、依赖、决策、证据及来源 |
| `awr_context_compile`，无 `branch` | `context compile --work WORK [--session SESSION] [--detached]` | L1、完整性、缺口、来源、预算和 Context hash |
| `awr_context_compile`，显式 `branch` | `branch context NAME_OR_ID --work WORK [--session SESSION] [--detached]` | 同一命名分支自 fork 起的增量；不切换默认分支 |
| `awr_work_transition` | `work progress/block/unblock/cancel/reopen WORK --session SESSION --reason TEXT --expected-revision R` | `action` 选择动作，`next_action` / `summary` / `blocker` 对应同名连字符参数；返回提案、源写入结果和事件回执 |
| `awr_work_transition`，`action=complete` | `work complete WORK --session SESSION --reason TEXT --input /absolute/completion.json --expected-revision R` | MCP `completion` 是文件里的 JSON 对象；逐条绑定验收与证据，成功后释放本会话的认领 |
| `awr_event_append` | `event append --type TYPE --summary TEXT --expected-revision R` | `event_type` 对应 `--type`；可带 `--work`、`--session`、`--branch`、`--importance`、`--payload FILE`；返回完整 Event |
| `awr_evidence_record` | `evidence add --input FILE --expected-revision R` | 输入字段映射见下节；返回完整 Evidence 与 `event_id` |
| `awr_search` | `search [TEXT] [--type KIND] [--status STATUS] [--work WORK] [--limit N]` | `text` 对应位置参数，`kind` 对应 `--type`，其余同名；`work` 是 `work_item` 的别名 |

两种 Context 入口均支持 `work`、`session`、`detached`、`agent`、`goals`、`paths`、`tags`、`source_sha`、`intent`、`budget`；CLI 对应 `--work`、`--session`、`--detached`、`--agent`、可重复的 `--goal` / `--path` / `--tag`、`--source-sha`、`--intent`、`--budget`。省略的值采用同一领域默认值。

无显式 MCP `branch` 时，`checkpoint` / `after_revision` 对应 CLI `--checkpoint` / `--after-revision`，两者互斥。显式 MCP `branch` 使用 fork 基线，不接受这两个自定义基线。CLI 单独提供的 `context compile --branch ID` 使用普通上下文基线；它与 `branch context NAME_OR_ID` 的含义不同，不用来替代上表中的命名分支映射。

查询的 `branch` 可以是名字、内部 ID 或 `main`；省略时读取项目当前默认分支。显式选择不会修改默认分支、既有会话、认领或 Git checkout。MCP 的 `null` 等同省略，选择主分支须传 `"main"`。

## 记录输入与回执

`awr_evidence_record` 与 CLI EvidenceDraft 的字段如下：

| MCP 参数 | CLI 输入 JSON 字段 |
| --- | --- |
| `expected_revision` | 命令行 `--expected-revision`，不写进输入 JSON |
| `work` | `work_item_key` |
| `branch` | `branch_id`，传内部 ID；显式 `null` 表示主分支，省略表示当前分支 |
| `external_key`, `evidence_type`, `level`, `summary`, `locator`, `sha256`, `source_sha`, `command`, `scope`, `verified_at` | 同名 |

两端成功写入都返回 `ok`、`project_revision`、`evidence`、`event_id`、`validation_basis`、`freshness_basis`、`source_refresh_performed`。`evidence.evidence_type`、`summary`、`command`、`scope` 是实际保存的完整值，不截短调用者的记录。`validation_basis=caller_supplied_bindings` 表示按调用者提交的绑定保存；记录成功不代表命令被执行或业务验收已完成。

这修正了早期 CLI `evidence add` 用摘要字段 `type` 返回写入结果的行为。读取命令 `evidence show` 继续提供原有摘要视图（含 `type`、`scope_total`）；它不是写入回执。

`event append` 的 `--payload FILE` 是 JSON 值文件，省略时为 `{}`。MCP 直接传 `payload` JSON 值。`importance` 默认 `normal`。成功回执含 `ok`、`project_revision`、完整 `event` 和刷新元数据。事件 ID、时间由每次实际执行产生；独立的两次写入不会得到同一个 ID。

通用事件不能伪造 `work.completed` 等保留的领域事件；必须走对应状态变更服务。Event payload、Evidence command 都只是记录内容，不作为命令执行。

Evidence 输入和 Event payload 的相对路径以 `--project` 根目录为基准。Completion 输入的相对路径以进程当前目录为基准，自动化建议始终使用绝对路径。

## 版本、来源与完整性

在同一项目、同一分支选择、同一已索引来源和相同参数下，两个入口返回相同的领域状态、标识、版本、验收、依赖、诊断及缺口。查询中的 `active_claims` 都包含 `id`、`agent_id`、`session_id`、`expires_at`。`revision` 是对象版本，`source_revision` 是来源版本，`project_revision` 是项目状态版本，三者不可互换。

读取策略有意保留以下差异：

| 字段 / 行为 | CLI status / ready / work show / search / L1 | MCP 的 5 个读工具 |
| --- | --- | --- |
| `freshness_basis` | `source_refresh` | `source_verified_readonly` |
| `source_refresh_performed` | `true` | `false` |
| `read_only` | `false`，可能更新可重建的 DB 投影 | `true`，不持久化任何表的变化 |
| 来源已改变 | 先刷新，返回新版本或显式缺口 | 返回 `SourceStale`；显式运行 `awr source reindex` 后再读 |

MCP 在内存快照中验证来源，并在返回前再次检查来源和真实 DB revision。它不会把临时快照版本当作持久状态返回。CLI 刷新不会改写权威源文件。通用查询提供 `source_issues` 和 `source_warnings`；L1 将来源与所需缺口放在自己的上下文和 `completeness` 中。

因此，对照读取应先同步来源，再比较同一个 `project_revision`，仅区别表中的三个传输策略字段；不能忽略 revision、Context hash、硬事实或缺口来制造一致结果。CLI 刷新失败时可能同时返回带 `source_issues` 的诊断正文和错误；MCP 拒绝陈旧快照时只返回错误，不返回一个可执行的旧结果。

Event / Evidence 写入在刷新前后都核对 `expected_revision`。刷新前已过期的请求不执行刷新或领域写入；刷新发现来源变化而推进版本后，拒绝沿用旧版本追加记录。状态变更使用同一领域提案与源指纹校验流程。

L1 JSON 顶层包含 `ok` 和 `project_revision`；有必需缺口时为 `ok=false`，同时保留 `completeness`、诊断与可用正文，并带 `error.code=ContextIncomplete`。硬事实超预算时返回 `BudgetExceeded` 和 `details.required` / `details.budget`，不截断硬事实后宣称完整。

## 输出、错误与退出码

CLI 普通输出面向人阅读：默认列出有界摘要，状态页最多显示一个当前建议，ready 默认最多 10 项。事件追加默认显示 ID、类型、短摘要和版本；不回显 payload。`event show --full`、`object show --full` 和报告内容读取属于显式展开。L1 的默认文本输出是预算约束内的执行上下文，完整验收与硬规则保留。

`--json` 模式下，成功正文是 stdout 上的一个 JSON 对象。失败可能有可检查的部分正文；stderr 是独立的 typed error JSON，调用者必须分别解析两个流。不要用 `2>&1` 合并后再整体解析。

```json
{
  "code": "RevisionConflict",
  "message": "revision conflict: expected 10, actual 11",
  "details": {"expected": 10, "actual": 11}
}
```

上例仅展示形状。`code` 与 `details` 是机器判断依据，`message` 用于解释，不承诺不同参数解析器逐字一致。没有附加结构时省略 `details`。

| 情况 | CLI | MCP |
| --- | --- | --- |
| 成功 | exit 0；JSON 正文在 stdout | `isError=false` / 省略；`structuredContent` 与首个 text content 的 JSON 完全相同 |
| 领域错误 | exit 1；typed error 在 stderr | `isError=true`；相同 typed error 在 `structuredContent` 和 text content |
| 有诊断 / 持久提案的部分失败 | exit 1；正文在 stdout，typed error 在 stderr | `isError=true`；正文保留 `error`、缺口或提案回执 |
| CLI 缺参数、参数值错误、未知参数、未知嵌套子命令 | exit 2；`--json` 时 `InvalidInput` | 工具参数不符合 schema 时 `InvalidInput`、`isError=true` |
| 未实现的 CLI 顶层命令 | exit 1、`Unsupported` | 未知工具名是 JSON-RPC `-32601`（method not found），无领域结果 |
| `--help`、`--version` | exit 0；即使带 `--json` 也输出普通帮助 / 版本文本 | 使用 initialize / tools list 协议能力 |

`--json` 支持放在已知子命令前后。为了诊断未知顶层命令，放在命令前。`--` 之后的字面量 `--json` 和 `--field=--json` 不切换输出模式。

输入 JSON 语法或字段错误返回 `InvalidInput`，不冒充内部序列化错误。若同时存在多个错误，两端的参数解析与文件读取顺序可能不同；修正首个错误后再继续，不依赖多重故障的报错先后顺序。

核心错误保留 `NotFound`、`SourceUnavailable`、`SourceStale`、`SourceConflict`、`RevisionConflict`、`DependencyBlocked`、`ClaimConflict`、`RuleViolation`、`EvidenceMissing`、`MutationUnsupported`、`MutationConflict`、`ContextIncomplete`、`BudgetExceeded`、`InvalidTransition`。`InvalidInput`、`Unsupported`、`Storage`、`Io`、`Json` 表示输入、实现或底层错误。提案 / 恢复错误还可能包括 `proposal_required`、`MutationIncomplete`、`WorkActionIncomplete`、`CheckpointIncomplete`，其中附带的提案或尝试 ID 是恢复入口。

发现部分写入或通信中断时先检查提案、事件、会话与当前版本，再决定下一步；不能看到进程失败就盲目重试写入。MCP 进程无法绑定项目或协议本身损坏时属于启动 / 协议错误，没有可以读取的领域 `CallToolResult`。

MCP 参数总量最多 1 MiB；CLI Evidence 输入 / Event payload 文件最多 1 MiB；Completion 映射最多 64 KiB。领域字段、预算和内容读取另有自己的上限，外层限制不放宽内层校验。

## 定向验证

```bash
cargo build -p awr-cli -p awr-mcp --locked
python3 crates/awr-mcp/tests/cli_parity.py \
  --awr target/debug/awr --mcp target/debug/awr-mcp
cargo test -p awr-cli --test output_cli --locked
cargo test -p awr-mcp --test stdio --locked
```

对照程序使用临时 fixture，实际启动 CLI 与 MCP stdio。读取比较完整领域 JSON，逐表检查 MCP 未写入 DB，检查权威来源字节不变；写入从同一快照分别执行，并核对状态、源文件结果、revision、缺口、证据和认领回执。Context hash 和必需事实不做归一化。独立写入的生成 ID / 时间分别验证，不能作为两次执行内容相等的条件。

这些属于本地接口与功能验证，不代表真实客户端业务验收、性能测量或发布结果。
