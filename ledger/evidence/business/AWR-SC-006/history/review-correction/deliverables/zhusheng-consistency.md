# 竹声维护交接一致性说明

> 文档性质：当前执行者形成的交接说明，不是独立复核结论。前部保留 AWR project revision 73、权威来源 revision 6 时的历史快照；文末“独立复核后整改状态”是当前结论，基准为 project revision 174、权威来源 revision 17、Git `HEAD` `8218b29c348b7eda3e57b541c3489e41b87b6679`。

## 整改前结论摘要（历史快照：revision 73）

- 已保存且已进入 Git 基线：原交接草案和三条变更请求。
- 已保存但尚未进入 Git：新增归档确认、角色确认、诊断/恢复/提案/本一致性说明以及 AWR 本地回执。
- 已在权威台账生效：`ZS-DIAG` 与 `ZS-COMPARE` 已完成，`ZS-RECOVER` 已进入 `in_progress`。这些是工作流程状态，不代表三条旧业务 patch 已应用。
- 三份旧原子提案均已显式标记为 `rejected`，每个 ID 在库存中只出现一次，且均无 apply attempt；其内容和回执没有删除。
- 新增归档确认补足了三个目录名称，角色确认补足了角色级职责边界；具体遗留问题仍缺事实。`ZS-RECOVER.acceptance` 仍只有原来的两条，没有重复追加建议。

## 整改前保存与生效矩阵（历史快照）

| 修改或产物 | 保存状态 | 权威/业务生效状态 | 当前证据与说明 |
| --- | --- | --- | --- |
| `materials/handover-draft.md` | 已保存并进入 Git 基线 | 草案本身不是权威台账变更 | `materials/handover-draft.md:3`；SHA-256 `4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361` |
| `materials/change-requests.csv` 三条建议 | 已保存并进入 Git 基线 | 仅为建议，未因此写入 `ZS-RECOVER` | `materials/change-requests.csv:2-4`；SHA-256 `0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2` |
| `materials/archive-confirmation.md` | 已落盘，当前 Git 状态为未跟踪 | 未进入权威台账，也未批准或应用 CR-01 | `materials/archive-confirmation.md:2`；SHA-256 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c` |
| `materials/role-confirmation.md` | 已落盘，当前 Git 状态为未跟踪 | 未进入权威台账，也未批准或应用 CR-02 | `materials/role-confirmation.md:2`；SHA-256 `8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5` |
| `ZS-DIAG` 流程状态 | 已写入 `work-ledger.yaml:6`，工作树未提交 | 已由 AWR 应用：`completed`、`locally_verified` | `work.completed` 事件 `01M21WAZT1HX58ASPY3G5X95R7`；`.work-receipts/zs-diag-current-history.json` |
| `ZS-COMPARE` 流程状态 | 已写入 `work-ledger.yaml`，工作树未提交 | 已由 AWR 应用：`completed`、`locally_verified` | `work.completed` 事件 `01M220936P02WQGAD119TQ246V`；`.work-receipts/zs-compare-history-after-completion.json` |
| `ZS-RECOVER` 流程状态 | 已写入 `work-ledger.yaml`，工作树未提交 | 已由 AWR 应用：`in_progress` | `work.progressed` 事件 `01M220DTPGKYR02FB2F11VCE5S`；`.work-receipts/zs-recover-history-current.json` |
| CR-01 资料归档提案 | 已保存在 AWR，ID `01M21X4H7GSP7DY19MTHYDBXST` | 已 `rejected` 为合并处理替代；未应用、无 apply attempt | `.work-receipts/cr-01-after-disposition.json` |
| CR-02 负责角色提案 | 已保存在 AWR，ID `01M21X5MHW7RFN8PBE4J86GACM` | 已 `rejected` 为合并处理替代；未应用、无 apply attempt | `.work-receipts/cr-02-after-disposition.json` |
| CR-03 遗留问题提案 | 已保存在 AWR，ID `01M21X67QFPQ55YSMJEX96HYV7` | 明细缺失；已 `rejected` 为合并处理替代，未应用、无 apply attempt | `.work-receipts/cr-03-after-disposition.json` |
| 首次 CR-01 创建尝试 | 失败回执已保存 | AWR 拒绝跨工作项会话绑定；没有创建提案、没有改动来源 | `.work-receipts/cr-01-create-session-mismatch-error.json` |
| 诊断、恢复和交接说明 | 已落盘，当前位于未跟踪的 `deliverables/` | 支撑核对，不替代权威源或独立复核 | `deliverables/zhusheng-interruption-diagnosis.md`、`deliverables/zhusheng-recovery-work.md`、本文件 |
| `.gitignore` 的 AWR 本地状态规则 | 已保存在工作树，尚未提交 | 只影响 Git 忽略行为，无业务状态效力 | `.work-receipts/zs-compare-git-diff.patch` |

## 三类业务事项的整改前交接

### 1. 资料归档

- 已确认到材料中的事实：资料管理员确认按“使用说明、交接记录、问题跟踪”三个目录归档，来源为 `materials/archive-confirmation.md:2`。
- 仍未知：目录根路径、访问权限、现有资料是否已迁移，以及迁移后的完整性核对结果。
- 生效边界：确认事实已进入交接说明；旧 CR-01 已拒绝且从未应用，仍不能写成实际归档迁移已完成。

### 2. 负责角色

- 已保存建议：按值班与资料分工，来源为 `materials/change-requests.csv:3`。
- 已确认到材料中的角色级边界：值班人员维护交接记录，资料管理员维护使用说明，问题负责人维护问题跟踪，来源为 `materials/role-confirmation.md:2`。
- 仍未知：具体人员名单、值班表、代理/升级路径和交接时限。
- 生效边界：确认事实已进入交接说明；旧 CR-02 已拒绝且从未应用。

### 3. 遗留问题

- 已保存建议：每项保留影响和下一步，来源为 `materials/change-requests.csv:4`。
- 角色材料再次要求未解决问题保留影响和下一步，但仍未提供具体问题清单、每项已知影响、负责人、下一步和完成条件。
- 生效边界：旧 CR-03 已拒绝且从未应用，具体问题仍必须补录。

## 整改前继续处理说明（已由文末当前状态取代）

1. 从活动会话 `01M220BJS3M8C7Y03WACQD54RM` 接续 `ZS-RECOVER`；写入前重新确认认领和来源指纹。
2. 将新增归档确认作为 CR-01 的支持材料审阅，同时补查目录根路径、权限和迁移状态；不要仅凭文件存在宣称归档已经执行。
3. 角色级职责边界已有协调者确认；后续补齐具体人员/值班信息，并向执行者取得遗留问题、影响和下一步，分别形成可追溯材料。
4. 三个旧提案已拒绝并保留，不得再次应用。若取得具体问题清单并需要新提案，只能基于最新来源创建一份去重后的变更。
5. 当前说明不签署独立复核。`ZS-REVIEW` 必须由不同执行者完成。

## 整改前未决项（历史快照）

- 新增归档确认尚未进入 Git，是否纳入正式提交尚未决定。
- 三份旧提案均已处置为拒绝且未应用；实际归档执行及完整性验证仍没有证据。
- CR-01/02 的确认事实已保存在交接说明；CR-03 所需的具体问题事实尚未提供。
- 当前工作树包含未提交的权威台账流程变更和多个未跟踪交付物；本次核对不代替提交或发布。
- 独立复核与最终交付均未发生。

## 独立复核后整改状态（当前）

### 当前权威状态

- 独立复核 `deliverables/zs-independent-review.md` 已发生并给出 `requires_executor_changes`；其原报告 SHA-256 `c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5` 保持不变。本执行者没有改写或代签该报告。
- `ZS-RECOVER` 已因复核缺陷在 project revision 111 重开，经整改证据 `zs-recover-remediation-20260909-v1` 重新于 revision 141 完成。当前 acceptance 是四条且互不重复：原两条、已确认的归档要求、已确认的角色要求。
- 新合并提案 `01M225AVE1BFYY0W4T2EKRV88C` 基于 source revision 11 创建，只纳入 CR-01/CR-02；它在 project revision 126 成功应用一次，写后指纹为 `sha256:83f9e1a0a4cb7b598e5ac0e60b712273b4c85f8e75a246ad59b11225c8ba17a0`。
- 之后的 `work.progress`、`work.complete`、`work.reopen` 及两次交接元数据同步只更新流程字段。当前权威来源 revision 17、整体指纹 `sha256:910155627fdf6131e7aed88c5297cc9c61a97c60aa4beec9a7c08475de8e4a37`，四条 acceptance 仍与新提案写入结果完全一致。
- 旧独立复核证据早于整改来源，因此 `ZS-REVIEW` 已在 project revision 152 重开为 `planned`，当前可由不同参与者认领返检。`ZS-DELIVER` 因此处于依赖阻断状态；`deliverables/zs-delivery.md` 尚不存在。

### 当前保存与生效矩阵

| 项目 | 保存状态 | 当前生效状态 | 证据边界 |
| --- | --- | --- | --- |
| 原草案与三条原建议 | 保留，哈希未变 | 原建议文本本身不直接构成权威应用 | `materials/handover-draft.md`；`materials/change-requests.csv` |
| 归档确认 | 保留，哈希 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c` | 目录名称与确认来源已写入当前 acceptance 一次 | `materials/archive-confirmation.md`；新合并提案 |
| 角色确认 | 保留，哈希 `8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5` | 角色级职责与确认来源已写入当前 acceptance 一次 | `materials/role-confirmation.md`；新合并提案 |
| 三份旧原子提案 | ID、immutable patch、失败和拒绝回执全部保留 | 各自 `rejected`、`apply_attempt: null`，应用 0 次 | 旧建议未重放，不等于新修订未生效 |
| 新合并提案 | ID `01M225AVE1BFYY0W4T2EKRV88C` 与完整生命周期保留 | `applied`，唯一 apply attempt `01M225BCXTY18AWZSRG0B9WT1P` | 新修订已生效 1 次，不证明归档迁移或人员排班已执行 |
| CR-03 具体遗留问题 | 原建议和旧提案保留 | 未进入新 acceptance | 具体问题、影响、负责人、下一步和完成条件仍缺事实 |
| 失败历史 | 原生产者、复核者与本次整改失败文件均保留 | 失败调用均未改业务来源 | `.work-receipts/cr-01-create-session-mismatch-error.json`、`reviewer-tool-failures.json`、`zs-remediation-tool-failures.json`、`zs-recovery-final-doctor.log` |
| Git 状态 | 工作树内容仍未形成新提交 | 不得表述为已提交、已发布或 E4 | Git `HEAD` 仍为 `8218b29c348b7eda3e57b541c3489e41b87b6679` |

### 当前未决项与下一步

- 仍未知：目录根路径、访问权限、资料迁移与完整性；具体人员、值班表、代理/升级路径和交接时限；具体遗留问题清单及逐项影响、负责人、下一步和完成条件。若没有遗留问题，也须有执行者明确确认“无”。
- 下一步只能由不同参与者认领 `ZS-REVIEW`，固定整改后文件与来源哈希，逐项返检 ZS-F01、ZS-F02、ZS-F03，并保留原报告、追加返检结论。
- 返检通过前不推进 `ZS-DELIVER`；即便返检通过，也只能据实际证据形成最终交接与未决事项，不能把本地校验扩大为 Git 提交、发布或 E4。
- `ZS-RECOVER` 的当前 `summary`/`next_action` 已由提案 `01M226DVJZNTTSTGZ20CHGQDWD` 对齐为“整改已提交、等待返检”；`ZS-REVIEW.summary` 已由提案 `01M226HF1GX6P579T58HV384G8` 对齐为“原报告保留、返检尚未发生”。两案均不包含 acceptance 字段。
