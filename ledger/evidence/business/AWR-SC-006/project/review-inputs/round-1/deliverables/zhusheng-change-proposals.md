# 竹声维护交接变更提案索引

> 文档性质：待审核提案索引，不是权威台账，也不是审批或应用回执。三份提案当前均为 `ready`；不得据此宣称建议已确认、已生效或已完成独立复核。

> 后续事实更新：`materials/archive-confirmation.md:2` 已新增资料管理员确认，给出“使用说明、交接记录、问题跟踪”三个目录名称，同时明确负责角色仍待协调者确认。该文件已落盘但未进入 Git；CR-01 仍未批准、未应用。最新保存/生效分层见 `deliverables/zhusheng-consistency.md`。

## 保留基线

- 已保存整理稿 `deliverables/zhusheng-interruption-diagnosis.md` 保持不变，SHA-256 为 `a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196`。
- 原始建议仍来自 `materials/change-requests.csv:2-4`；原始交接说明仍来自 `materials/handover-draft.md:3`，两份材料均未改动。
- 后续新增的 `materials/archive-confirmation.md` SHA-256 为 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c`；它补充目录名称，但没有给出目录根路径、权限或迁移完成情况。
- 三份提案均绑定 `ZS-RECOVER` 在 source revision 4 的同一源指纹：`sha256:83d49fc17c8d12f487c59687df314093e8d3c360bf55847b9b5a98cfb8d0abf8`。
- 当前 `ZS-RECOVER` 仍只有原来的两条验收条件，以下追加内容尚未写入 `work-ledger.yaml`。

## 待审核提案

| 编号 | AWR 提案 ID | 建议追加到 `ZS-RECOVER.acceptance` 的内容 | 确认责任 | 状态 |
| --- | --- | --- | --- | --- |
| CR-01 资料归档 | `01M21X4H7GSP7DY19MTHYDBXST` | 资料归档按内容类型分目录；三个目录的具体名称或位置及资料类型对应关系须经资料管理员确认后才可应用。 | 资料管理员 | 确认材料已保存；提案仍为 `ready`，未批准、未应用 |
| CR-02 负责角色 | `01M21X5MHW7RFN8PBE4J86GACM` | 负责角色按值班与资料分工；具体角色与职责边界须经协调者确认后才可应用。 | 协调者 | `ready`，未批准、未应用 |
| CR-03 遗留问题 | `01M21X67QFPQ55YSMJEX96HYV7` | 遗留问题保留已知影响和下一步；具体问题清单须经执行者核对后才可应用。 | 执行者 | `ready`，未批准、未应用 |

## 逐项确认与接续规则

1. 审核时按提案 ID 读取 immutable patch，分别记录确认、退回或待补，不以本索引代替 AWR 提案正文。
2. 三个目录名称已有新增材料支持，但目录根路径、权限和迁移状态仍未知；角色分配与遗留问题明细也仍未知。后续处理必须保留事实来源，不能只确认建议措辞。
3. 三份提案绑定同一个目标记录和同一个源指纹。若确认多项，不得连续应用这些旧提案；应先汇总已确认项，再基于届时最新源状态创建一份合并提案，并将原子提案明确拒绝或标记为被替代。
4. 在新的合并提案完成审核前，不执行 `approve` 或 `apply`，也不直接编辑 `work-ledger.yaml`。
5. AWR 会在审核和应用前复核完整来源指纹；后续任何 `work-ledger.yaml` 写入都可能使这些提案失去当前绑定，接续前必须先检查 `binding_valid`，不得绕过冲突重放。

## 回执边界

- 首次尝试把 `ZS-RECOVER` 提案绑定到 `ZS-COMPARE` 会话时，AWR 以 `work target conflicts with the creating session` 拒绝，未创建提案、未改动来源；失败回执已保留。
- 随后的三份提案按 AWR 允许的无创建会话绑定方式创建，并由当前执行者提交审核。`created_by_session: null` 是该约束的结果，不代表缺少提案事件或本地回执。
- 本索引只提供可读导航。审批者身份仍是调用者声明，不能单独作为独立复核证据。
