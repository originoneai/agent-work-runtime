# 竹声维护交接恢复及续做记录

> 文档性质：中断恢复记录，不是权威台账、提案批准、应用回执或独立复核结论。记录时间为 2026-09-09T09:52:49+08:00。

## 恢复结论

- 中断前的原始材料、诊断稿、提案索引、失败回执和三份 AWR 提案均已保留，当前校验未发现内容漂移或重复创建。
- 三份提案仍为 `ready` 且 `binding_valid: true`，均没有 apply attempt；它们只是待审核建议，尚未批准、尚未应用到 `work-ledger.yaml`。
- 原活动会话 `01M21WZXPSGZ77369CSGXCB8CX` 已由 AWR 原子标记为 `interrupted`，其旧认领已关闭；续接会话 `01M21XP8VZ1YJRNSS6BJH2V4HB` 已处于 `active`，持有 `ZS-COMPARE` 的新认领 `01M21XP8VZ19C36AM6A0HG2QRW`。
- 续接后的 bootstrap 与完整执行上下文均为 complete；本次实际使用的完整上下文哈希为 `ab4beb6621addd2f7e510eefea145ea90b86d4c19b0c6cd138d7600c8acae841`。
- 原会话没有成功 checkpoint。AWR 因而只能按当前来源和会话起始修订恢复；任何未落盘的摘要、下一动作或开放循环无法从 AWR 还原，继续工作时不得臆测补写。

## 已保存结果核对

| 层级 | 当前事实 | 核验依据 |
| --- | --- | --- |
| Git 基线 | `HEAD` 仍为 `8218b29c348b7eda3e57b541c3489e41b87b6679`；两份原始材料已在该基线中。 | `git rev-parse HEAD`；`deliverables/zhusheng-interruption-diagnosis.md` |
| 原始材料 | `materials/handover-draft.md` 与 `materials/change-requests.csv` 的 SHA-256 分别仍为 `4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361`、`0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2`。 | `.work-receipts/preserved-handover-inputs.sha256`；恢复时重新执行哈希校验 |
| 诊断与提案索引 | 诊断稿哈希仍为 `a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196`；提案索引已保存且逐项包含三个提案 ID。 | `deliverables/zhusheng-interruption-diagnosis.md`；`deliverables/zhusheng-change-proposals.md` |
| AWR 权威来源 | `work-ledger.yaml` 当前来源修订为 4，指纹为 `sha256:83d49fc17c8d12f487c59687df314093e8d3c360bf55847b9b5a98cfb8d0abf8`；三条建议文本尚未写入 `ZS-RECOVER.acceptance`。 | `.work-receipts/zs-compare-status-after-resume.json`；`.work-receipts/zs-recover-after-proposals.json` |
| 待审核提案 | `01M21X4H7GSP7DY19MTHYDBXST`、`01M21X5MHW7RFN8PBE4J86GACM`、`01M21X67QFPQ55YSMJEX96HYV7` 均为 `ready`、绑定有效。 | `.work-receipts/zs-compare-ready-proposals-after-resume.json`；逐项 `proposal show --full` 核对 |
| 失败现场 | 首次跨工作项会话绑定创建提案被 AWR 拒绝，未创建提案、未改来源；失败记录没有被抹去。 | `.work-receipts/cr-01-create-session-mismatch-error.json` |

## Git 与运行态边界

- 当前工作树并非干净提交：`.gitignore` 和 `work-ledger.yaml` 有已跟踪改动；`.awr/`、`AGENTS.md`、`DELIVERABLES.md` 与 `deliverables/` 仍显示为未跟踪路径。这些现场均被保留，本次恢复没有清理、覆盖、暂存或提交它们。
- `.work-receipts/` 由 `.git/info/exclude` 精确排除，回执仍在本地保存；该排除规则没有在本次恢复中扩大。
- AWR runtime 状态与 Git 提交状态是两条边界：会话续接成功不代表业务建议已写入来源，也不代表当前工作树已形成提交。

## 可继续推进的起点

1. 继续前先读取续接会话 `01M21XP8VZ1YJRNSS6BJH2V4HB` 的最新 checkpoint，并确认其认领仍有效；认领过期时按 AWR 规则重新取得，不并发创建第二个写入者。
2. 按提案 ID 读取 immutable patch，向资料管理员、协调者和执行者分别取得缺失事实与来源；当前未知项仍是三个目录的具体值、角色与职责边界、遗留问题明细及影响/下一步。
3. 未取得这些确认前，不批准、不应用现有提案，也不直接编辑 `work-ledger.yaml`。
4. 若多项得到确认，按 `deliverables/zhusheng-change-proposals.md` 的接续规则，以届时最新来源创建合并提案，并明确处置现有原子提案，避免连续应用同一旧指纹上的覆盖式变更。
5. 独立复核必须由不同执行者完成；本记录不能代替 `deliverables/zs-independent-review.md`。

## 续做结果（2026-09-09）

本节记录恢复后的后续事实；前文保留为中断恢复时点的原始快照，不回写覆盖。

- 新增输入已保留：`materials/archive-confirmation.md` 确认“使用说明、交接记录、问题跟踪”三个目录；`materials/role-confirmation.md` 确认值班人员、资料管理员和问题负责人的角色级维护边界。
- `ZS-COMPARE` 已在 project revision 59 通过 AWR 完成，完成事件为 `01M220936P02WQGAD119TQ246V`，对应证据和完成回执均已保存。
- 三份旧原子提案分别在 project revision 64、65、66 被拒绝为“由最新来源上的合并处理替代”。每个原提案 ID 在库存中只出现一次，三份 `proposal show --full` 均为 `apply_attempt: null`；因此没有顺序覆盖、重复应用或静默丢弃。
- 原始 patch、创建/提交回执、首次失败回执和拒绝回执均继续保存在 `.work-receipts/`。拒绝只改变提案生命周期，不删除其内容。
- `ZS-RECOVER` 已由会话 `01M220BJS3M8C7Y03WACQD54RM` 认领，并在 project revision 73 进入 `in_progress`。当前 `work-ledger.yaml` SHA-256 为 `72ea722ab66dfd3b45b46a1a8cac3be96fb4f8ee3b411d939a4fff760b58ed0c`。
- 当前没有创建新的合并业务提案：CR-03 所需的具体遗留问题清单仍未提供，不能为了结束流程而虚构或应用不完整内容。

### 去重与防丢失核对

| 核对项 | 结果 | 回执 |
| --- | --- | --- |
| 三个旧提案 ID 唯一性 | 每个 ID 恰好一条库存记录 | `.work-receipts/zs-recover-proposal-inventory.json` |
| 旧提案应用次数 | 三项均无 apply attempt | `.work-receipts/cr-01-after-disposition.json`、`cr-02-after-disposition.json`、`cr-03-after-disposition.json` |
| 旧提案处置 | 三项均为 `rejected`，原因明确为合并去重或缺少问题事实 | `.work-receipts/cr-01-rejected-superseded.json`、`cr-02-rejected-superseded.json`、`cr-03-rejected-superseded.json` |
| 比较工作完成 | 恰有一条 `work.completed` 事件 | `.work-receipts/zs-compare-history-after-completion.json` |
| 恢复工作推进 | 恰有一条 `work.progressed` 事件 | `.work-receipts/zs-recover-history-current.json` |
| 原材料完整性 | 原草案、变更请求与诊断稿哈希保持不变 | `.work-receipts/preserved-handover-inputs.sha256` |

### 尚待外部输入

- 执行者提供具体遗留问题清单，并为每项给出已知影响、负责人、下一步和完成条件；如果确实没有遗留问题，也必须由执行者明确确认“无”。
- 不同执行者完成独立复核并形成 `deliverables/zs-independent-review.md`。本执行者不代签。
