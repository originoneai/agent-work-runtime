# 实际客户端验收准备

2026-09-09 的本轮准备绑定 AWR 合同 1.0.1。八套项目已分别生成新的 namespace、工作图来源和 Git 输入基线；完整绑定见 [准备记录](preparation-20260909-v1.json)。这批材料尚未执行真实客户端业务，E4 仍为 **0/8**。

本地目录前缀为 `.local/business-live-20260909-v1/`。每个目录中的 `project/` 是客户端工作区；`run.json` 是准备回执；`execution-record.json` 保留尚未填写的实际轮次、复核与交付记录。输入 seed commit 不属于业务交付提交。

| 场景 | 本地目录 | 首个自然业务输入 | 额外执行条件 |
| --- | --- | --- | --- |
| 接手新项目 | `project-onboarding/project/` | [初始请求](../../../tests/fixtures/business/project-onboarding/prompts/initial.md) | 执行者与独立复核者 |
| 中断后继续长任务 | `long-task-resume/project/` | [前序工作](../../../tests/fixtures/business/long-task-resume/prompts/prelude.md) | 前序工作与 checkpoint 必须真实产生，随后由新会话接续 |
| 外部来源变化 | `source-change/project/` | [初始请求](../../../tests/fixtures/business/source-change/prompts/initial.md) | 两次来源变化按实际业务进展发布 |
| 并行分工 | `parallel-ownership/project/` | [前序分工](../../../tests/fixtures/business/parallel-ownership/prompts/prelude.md) | 两位实际执行者及可追溯的所有权冲突 |
| 补齐证据后交付 | `evidence-completion/project/` | [初始请求](../../../tests/fixtures/business/evidence-completion/prompts/initial.md) | 实际使用证据与独立复核 |
| 意外退出后恢复 | `interrupted-write/project/` | [前序工作](../../../tests/fixtures/business/interrupted-write/prompts/prelude.md) | 实际产物、待应用建议和可追溯中断 |
| 受限资料与大日志 | `restricted-material/project/` | [初始请求](../../../tests/fixtures/business/restricted-material/prompts/initial.md) | 按授权使用资料，第一轮再发布公开长日志 |
| 跨客户端接续分支 | `branch-handoff/project/` | [前序工作](../../../tests/fixtures/business/branch-handoff/prompts/prelude.md) | 不同实际客户端、分支交接和原生 MCP 工具调用 |

每套项目的 `control/client-setup/` 已生成绑定准确路径的 Codex/Kimi 配置草稿，尚未安装。拟使用 Codex CLI 执行业务，Kimi Code 参与跨客户端交接；并行执行及独立复核仅使用获准的 `luna_worker`。本机可执行文件版本已记录，但模型调用、客户端原生 MCP 连接、项目信任与参与者身份尚未验证。

项目 [AGENTS.md](../../../AGENTS.md) 要求显式授权后才能分派子代理。此次准备未启动其他模型客户端或子代理；下一步需落实上述执行方式的授权，再通过客户端正常的项目信任流程连接 AWR。保留既有模型设置及 hooks，不使用信任绕过参数。

执行时只提交链接文件中的自然业务文字。评判材料、内部编号和后续输入留在操作方；首轮结果保留并暂停编辑后才发布第一轮补充，再完成第二轮。每个场景由与产物作者不同的实际复核者核对后，单独提交交付物并核实远端 SHA。操作流程和证据要求见 [验收约定](../../../docs/acceptance/README.md)。

原 QA-002 的准备证据保持历史源码绑定。本次仅同步业务准备合同与运行记录模板的范围引用，八个业务定义、两轮追问、覆盖矩阵和证据硬门槛均未改变。
