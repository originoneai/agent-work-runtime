# 竹声维护交接变更提案索引

> 文档性质：提案历史与处置索引，不是权威台账、业务应用回执或独立复核结论。三份原子提案曾为 `ready`，现均已因合并去重需要标记为 `rejected`；原 patch 和事件历史继续保留。

> 后续事实更新：`materials/archive-confirmation.md:2` 已新增资料管理员确认，给出“使用说明、交接记录、问题跟踪”三个目录名称，同时明确负责角色仍待协调者确认。该文件已落盘但未进入 Git；CR-01 仍未批准、未应用。最新保存/生效分层见 `deliverables/zhusheng-consistency.md`。

> 再次更新：`materials/role-confirmation.md:2` 已新增协调者确认，明确值班人员、资料管理员和问题负责人的维护边界。该文件同样只在工作树落盘；CR-02 仍未批准、未应用，具体遗留问题清单仍未提供。

## 保留基线

- 已保存整理稿 `deliverables/zhusheng-interruption-diagnosis.md` 保持不变，SHA-256 为 `a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196`。
- 原始建议仍来自 `materials/change-requests.csv:2-4`；原始交接说明仍来自 `materials/handover-draft.md:3`，两份材料均未改动。
- 后续新增的 `materials/archive-confirmation.md` SHA-256 为 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c`；它补充目录名称，但没有给出目录根路径、权限或迁移完成情况。
- 后续新增的 `materials/role-confirmation.md` SHA-256 为 `8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5`；它补充角色级职责边界，但没有具体人员名单或遗留问题明细。
- 三份提案均绑定 `ZS-RECOVER` 在 source revision 4 的同一源指纹：`sha256:83d49fc17c8d12f487c59687df314093e8d3c360bf55847b9b5a98cfb8d0abf8`。
- 当前 `ZS-RECOVER` 仍只有原来的两条验收条件，以下追加内容尚未写入 `work-ledger.yaml`。

## 已处置的原子提案

| 编号 | AWR 提案 ID | 建议追加到 `ZS-RECOVER.acceptance` 的内容 | 确认责任 | 状态 |
| --- | --- | --- | --- | --- |
| CR-01 资料归档 | `01M21X4H7GSP7DY19MTHYDBXST` | 资料归档按内容类型分目录；三个目录的具体名称或位置及资料类型对应关系须经资料管理员确认后才可应用。 | 资料管理员 | 确认材料已保存；旧覆盖式提案已 `rejected`，无 apply attempt |
| CR-02 负责角色 | `01M21X5MHW7RFN8PBE4J86GACM` | 负责角色按值班与资料分工；具体角色与职责边界须经协调者确认后才可应用。 | 协调者 | 确认材料已保存；旧覆盖式提案已 `rejected`，无 apply attempt |
| CR-03 遗留问题 | `01M21X67QFPQ55YSMJEX96HYV7` | 遗留问题保留已知影响和下一步；具体问题清单须经执行者核对后才可应用。 | 执行者 | 明细仍缺失；旧覆盖式提案已 `rejected`，无 apply attempt |

## 逐项确认与接续规则

1. 三份旧提案的 immutable patch、创建/提交回执和拒绝回执均须保留；不以本索引替代 AWR 正文。
2. 三个目录名称和角色级职责边界已有新增材料支持，但目录根路径、权限、迁移状态、具体人员名单及遗留问题明细仍未知。后续处理必须保留事实来源，不能只确认建议措辞。
3. 三份旧提案均覆盖同一个 acceptance 数组，不能连续应用；它们已分别以“由最新来源上的合并处理替代”为由拒绝，拒绝事件为去重处置，不删除内容。
4. 由于具体遗留问题清单尚未提供，本阶段没有创建或应用新的合并业务提案；确认过的归档和角色事实已写入交付说明，未知项继续显式保留。
5. 当前 `ZS-RECOVER` 已在 source revision 6 进入 `in_progress`，台账 SHA-256 为 `72ea722ab66dfd3b45b46a1a8cac3be96fb4f8ee3b411d939a4fff760b58ed0c`。任何后续提案都必须绑定届时最新来源，不得重放上述旧 patch。

## 回执边界

- 首次尝试把 `ZS-RECOVER` 提案绑定到 `ZS-COMPARE` 会话时，AWR 以 `work target conflicts with the creating session` 拒绝，未创建提案、未改动来源；失败回执已保留。
- 随后的三份提案按 AWR 允许的无创建会话绑定方式创建，并由当前执行者提交审核。`created_by_session: null` 是该约束的结果，不代表缺少提案事件或本地回执。
- 三份提案分别在 project revision 64、65、66 被标记为 `rejected`；对应回执为 `.work-receipts/cr-01-rejected-superseded.json`、`.work-receipts/cr-02-rejected-superseded.json`、`.work-receipts/cr-03-rejected-superseded.json`。逐项 `proposal show --full` 均确认 `apply_attempt: null`。
- 本索引只提供可读导航。审批者身份仍是调用者声明，不能单独作为独立复核证据。

## 独立复核后合并整改提案

独立复核指出：前三份旧提案的 `apply_attempt: null` 只能证明没有重放，不能证明已经确认的 CR-01/CR-02 已生效。下列新提案是在最新来源上重新形成的精确合并修订；本节更新当前状态，前文保留为整改前历史。

| AWR 提案 ID | 纳入内容 | 未纳入内容 | 当前状态与应用证据 |
| --- | --- | --- | --- |
| `01M225AVE1BFYY0W4T2EKRV88C` | 已确认的三个归档目录及资料管理员来源；已确认的三类角色职责及协调者来源；原两条 acceptance 与相关未知边界 | CR-03 的具体问题要求，因为问题清单及逐项事实仍缺失 | `applied`；唯一 `apply_attempt` 为 `01M225BCXTY18AWZSRG0B9WT1P`，应用事件 `01M225BCYBY571X8FPBY8TGME8` |

- 写前来源 revision 11、指纹 `sha256:6ac1624435b6b626e32af360e94acb9bbcd406e9a2dfe0e3bf3926474fac612a`；写后来源 revision 12、指纹 `sha256:83f9e1a0a4cb7b598e5ac0e60b712273b4c85f8e75a246ad59b11225c8ba17a0`。
- 新提案在 project revision 120/121/122 完成创建、提交和批准，在 revision 126 完成应用。调用者批准记录是整改流程记录，不冒充整改后的独立返检。
- 三份旧提案 ID、patch、失败和拒绝历史不变，且各自仍为 `rejected`、`apply_attempt: null`；它们没有被恢复、批准或应用。
- 因此当前必须使用两种不同表述：**旧建议未重放（旧 patch 应用 0 次）**；**新修订已生效（新合并提案应用 1 次）**。
- 后续流程写回没有改变四条 acceptance。当前来源 revision 17、指纹 `sha256:910155627fdf6131e7aed88c5297cc9c61a97c60aa4beec9a7c08475de8e4a37`；`ZS-RECOVER` 已重新完成，`ZS-REVIEW` 已重开等待整改后独立返检。
- 提案 `01M226DVJZNTTSTGZ20CHGQDWD` 只同步 `ZS-RECOVER.summary` 与 `next_action`，提案 `01M226HF1GX6P579T58HV384G8` 只同步 `ZS-REVIEW.summary`；二者的字段列表均不含 `acceptance`，不能计作第二次业务修订应用。
