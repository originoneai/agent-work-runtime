# 竹声维护交接一致性说明

> 文档性质：当前执行者形成的交接说明，不是权威台账、提案审批、应用回执或独立复核结论。核对基准为 AWR project revision 48、权威来源 revision 4、Git `HEAD` `8218b29c348b7eda3e57b541c3489e41b87b6679`。

## 结论摘要

- 已保存且已进入 Git 基线：原交接草案和三条变更请求。
- 已保存但尚未进入 Git：新增归档确认、诊断/恢复/提案/本一致性说明以及 AWR 本地回执。
- 已在权威台账生效：`ZS-DIAG` 已完成并关联本地验证证据；`ZS-COMPARE` 已进入 `in_progress`。这两项是工作流程状态，不代表三条业务建议已经生效。
- 已提交待审但尚未生效：CR-01、CR-02、CR-03 三份提案均为 `ready`、绑定有效且没有 apply attempt；`ZS-RECOVER.acceptance` 仍只有原来的两条。
- 新增归档确认补足了三个目录名称，但尚未触发提案批准或台账写入；负责角色与遗留问题仍缺事实。

## 保存与生效矩阵

| 修改或产物 | 保存状态 | 权威/业务生效状态 | 当前证据与说明 |
| --- | --- | --- | --- |
| `materials/handover-draft.md` | 已保存并进入 Git 基线 | 草案本身不是权威台账变更 | `materials/handover-draft.md:3`；SHA-256 `4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361` |
| `materials/change-requests.csv` 三条建议 | 已保存并进入 Git 基线 | 仅为建议，未因此写入 `ZS-RECOVER` | `materials/change-requests.csv:2-4`；SHA-256 `0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2` |
| `materials/archive-confirmation.md` | 已落盘，当前 Git 状态为未跟踪 | 未进入权威台账，也未批准或应用 CR-01 | `materials/archive-confirmation.md:2`；SHA-256 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c` |
| `ZS-DIAG` 流程状态 | 已写入 `work-ledger.yaml:6`，工作树未提交 | 已由 AWR 应用：`completed`、`locally_verified` | `work.completed` 事件 `01M21WAZT1HX58ASPY3G5X95R7`；`.work-receipts/zs-diag-current-history.json` |
| `ZS-COMPARE` 流程状态 | 已写入 `work-ledger.yaml:7`，工作树未提交 | 已由 AWR 应用：`in_progress` | `work.progressed` 事件 `01M21X100BNG52MGXNTZD17VBA`；`.work-receipts/zs-compare-current-history.json` |
| CR-01 资料归档提案 | 已保存在 AWR，ID `01M21X4H7GSP7DY19MTHYDBXST` | `ready`；未批准、未应用、无 apply attempt | `.work-receipts/zs-compare-cr-01-current.json` |
| CR-02 负责角色提案 | 已保存在 AWR，ID `01M21X5MHW7RFN8PBE4J86GACM` | `ready`；未批准、未应用、无 apply attempt | `.work-receipts/zs-compare-cr-02-current.json` |
| CR-03 遗留问题提案 | 已保存在 AWR，ID `01M21X67QFPQ55YSMJEX96HYV7` | `ready`；未批准、未应用、无 apply attempt | `.work-receipts/zs-compare-cr-03-current.json` |
| 首次 CR-01 创建尝试 | 失败回执已保存 | AWR 拒绝跨工作项会话绑定；没有创建提案、没有改动来源 | `.work-receipts/cr-01-create-session-mismatch-error.json` |
| 诊断、恢复和交接说明 | 已落盘，当前位于未跟踪的 `deliverables/` | 支撑核对，不替代权威源或独立复核 | `deliverables/zhusheng-interruption-diagnosis.md`、`deliverables/zhusheng-recovery-work.md`、本文件 |
| `.gitignore` 的 AWR 本地状态规则 | 已保存在工作树，尚未提交 | 只影响 Git 忽略行为，无业务状态效力 | `.work-receipts/zs-compare-git-diff.patch` |

## 三类业务事项的当前交接

### 1. 资料归档

- 已确认到材料中的事实：资料管理员确认按“使用说明、交接记录、问题跟踪”三个目录归档，来源为 `materials/archive-confirmation.md:2`。
- 仍未知：目录根路径、访问权限、现有资料是否已迁移，以及迁移后的完整性核对结果。
- 生效边界：该确认可作为后续处理 CR-01 的输入，但 CR-01 当前仍是待审提案，不能写成台账已更新或归档已执行完毕。

### 2. 负责角色

- 已保存建议：按值班与资料分工，来源为 `materials/change-requests.csv:3`。
- 明确未确认：新增材料再次说明负责角色仍需协调者确认，未提供具体角色或职责边界。
- 生效边界：CR-02 尚未批准、尚未应用。

### 3. 遗留问题

- 已保存建议：每项保留影响和下一步，来源为 `materials/change-requests.csv:4`。
- 仍未知：具体问题清单、每项已知影响、负责人、下一步和完成条件。
- 生效边界：CR-03 尚未批准、尚未应用。

## 继续处理说明

1. 从活动会话 `01M21XP8VZ1YJRNSS6BJH2V4HB` 与 checkpoint `01M21Y1W5N3N6VEEC0AQGGT2N9` 接续；写入前重新确认认领和来源指纹。
2. 将新增归档确认作为 CR-01 的支持材料审阅，同时补查目录根路径、权限和迁移状态；不要仅凭文件存在宣称归档已经执行。
3. 向协调者取得具体角色与职责边界，向执行者取得遗留问题、影响和下一步，分别形成可追溯材料。
4. 三个旧提案共享 `work-ledger.yaml` 指纹 `sha256:83d49fc17c8d12f487c59687df314093e8d3c360bf55847b9b5a98cfb8d0abf8`。任何台账写入前都要重查 `binding_valid`；若指纹变化，应保留旧回执并基于最新来源重建或合并提案，不连续套用覆盖式旧 patch。
5. 当前说明可用于 `ZS-COMPARE` 的本地证据，但不签署独立复核。`ZS-REVIEW` 必须由不同执行者完成。

## 未决项

- 新增归档确认尚未进入 Git，是否纳入正式提交尚未决定。
- CR-01 的 AWR 审批与应用尚未发生；实际归档执行及完整性验证也没有证据。
- CR-02、CR-03 所需业务事实尚未提供。
- 当前工作树包含未提交的权威台账流程变更和多个未跟踪交付物；本次核对不代替提交或发布。
- 独立复核与最终交付均未发生。

