# 竹声中断恢复最终交付与未决事项

> 交付结论：竹声本次中断恢复及经独立返检确认的整改，已在当前项目内形成可继续维护的本地交接；`ZS-R04`、`ZS-R06` 与 `ZS-F01` 至 `ZS-F03` 已关闭。证据等级限于 `locally_verified`。本结论不表示 Git 已提交、归档已实际实施、具体人员已接岗、已经发布或整场 E4 已完成。

## 交付基线与边界

- 项目：`竹声维护交接`，AWR project ID `01M21VJFYF7E75EQ55DEH3D5TS`。
- Git 基线：`8218b29c348b7eda3e57b541c3489e41b87b6679`。当前恢复产物和权威台账改动仍在工作树中，本次交付未执行暂存、提交、推送或发布。
- 独立返检由 `luna_worker` 在 session `01M227QGYJ3BWHQBQWTX6GBAMW` 完成；生产整改和最终交付由 `codex-primary` 链路处理，两者身份可区分。
- 返检通过时 AWR 为 project revision 209、权威来源 revision 20，`ZS-REVIEW=completed`，`ZS-DELIVER=planned/ready`。本交付 session 为 `01M229PEPFG7KC962FP55GCRMX`；交付过程与最终状态以文末列出的 AWR 回执为准。
- 本文件是 `DELIVERABLES.md` 指定的“最终交付与未决事项”，不是归档迁移回执、人员排班表、发布证明或 E4 证明。

## 实际中断与恢复过程

| 阶段 | 已核实事实 | 可追溯依据 |
| --- | --- | --- |
| 首次 prelude | native return code 为 `0`，属于正常退出，不计为中断。 | `review-inputs/interruption-evidence/prelude-normal-exit-observation.json` |
| 实际中断前 | 同一 native task `01a083b6-aa5f-7ba0-9135-c128ada421a2` 已保存诊断、提案索引和三份 `ready` 提案；权威来源及两份产物在信号前已固定哈希。 | `review-inputs/interruption-evidence/actual-operator-interruption-v1.json` |
| 实际中断 | 操作方发送一次 `SIGKILL`，native return code 为 `-9`。自动观察器因提案 `created_by_session: null` 没有执行故障注入；该限制保留，不能反过来否认实际中断。 | `review-inputs/interruption-evidence/verified-interruption-binding.json` |
| 中断后保存状态 | 三份提案仍存在且保持 `ready`；权威来源和已落盘产物的前后哈希一致。 | `actual-operator-interruption-v1.json` 的 signal 前后快照 |
| AWR 恢复 | 旧 session `01M21WZXPSGZ77369CSGXCB8CX` 被原子标记为 `interrupted` 并释放旧 claim；新 native task `01a083d7-4073-7150-89cd-993a9899dcad` 以 session `01M21XP8VZ1YJRNSS6BJH2V4HB` 接续。 | `review-inputs/interruption-evidence/actual-recovery-binding.json`；`.work-receipts/zs-compare-interrupted-session-after-resume.json` |
| 恢复边界 | 旧 session 的 `last_checkpoint_id` 为 `null`。已落盘文件、提案和事件可以恢复；中断前未持久化的摘要、下一动作或开放循环无法还原。 | `actual-recovery-binding.json`；AWR session history |
| 初次独立复核 | 初次复核给出 `requires_executor_changes`，指出已确认的归档和角色要求尚未进入权威 acceptance，且恢复完成状态与下一步冲突。原报告未被覆盖。 | `deliverables/zs-independent-review.md`；SHA-256 `c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5` |
| 整改 | `ZS-RECOVER` 先重开；CR-01/CR-02 以一份新合并提案一次写入；随后重新完成并重开 `ZS-REVIEW` 返检。 | `.work-receipts/zs-remediation-evidence-report.json`；`.work-receipts/zs-recover-completion-receipt.json` |
| 独立返检 | 返检对固定整改包、当前来源和提案生命周期执行 247 项前置核验，结论为 `remediation_recheck_passed`；其后 51 项终态核验通过。 | `deliverables/zs-independent-re-review.md`；`.work-receipts/reviewer-recheck-verification.json`；`.work-receipts/reviewer-recheck-final-verification.json` |

## 已应用的确认要求

当前 `ZS-RECOVER.acceptance` 恰有四条：原始两条要求保持不变，以下两条确认要求各出现一次，并由同一新合并提案写入：

1. 资料归档按“使用说明、交接记录、问题跟踪”三个内容类型目录执行，并保留资料管理员确认来源 `materials/archive-confirmation.md`；目录根路径、权限、迁移与完整性未明确时继续列为未决。
2. 角色职责按协调者确认执行：值班人员维护交接记录、资料管理员维护使用说明、问题负责人维护问题跟踪，并保留确认来源 `materials/role-confirmation.md`；具体人员、值班表、代理/升级路径和交接时限未明确时继续列为未决。

新合并提案 `01M225AVE1BFYY0W4T2EKRV88C` 基于 source revision 11、写前指纹 `sha256:6ac1624435b6b626e32af360e94acb9bbcd406e9a2dfe0e3bf3926474fac612a` 创建；唯一 apply attempt 为 `01M225BCXTY18AWZSRG0B9WT1P`，应用事件为 `01M225BCYBY571X8FPBY8TGME8`，写后 source revision 12、指纹 `sha256:83f9e1a0a4cb7b598e5ac0e60b712273b4c85f8e75a246ad59b11225c8ba17a0`。

上述回执证明“确认要求已进入权威 acceptance 一次”，不证明目录已建立、资料已迁移、权限已配置或人员已排班。

## 原提案的处理结果与防重复结论

| 原提案 | 当前状态 | 应用次数 | 当前处理结果 |
| --- | --- | --- | --- |
| CR-01 `01M21X4H7GSP7DY19MTHYDBXST` | `rejected` | 0，`apply_attempt: null` | 原 patch、创建/提交/拒绝历史保留；已确认部分由新合并提案取代，没有重放旧 patch。 |
| CR-02 `01M21X5MHW7RFN8PBE4J86GACM` | `rejected` | 0，`apply_attempt: null` | 原 patch、创建/提交/拒绝历史保留；已确认部分由新合并提案取代，没有重放旧 patch。 |
| CR-03 `01M21X67QFPQ55YSMJEX96HYV7` | `rejected` | 0，`apply_attempt: null` | 原建议和历史保留；因具体问题事实缺失，没有纳入新合并提案，也没有被追认为完成。 |

必须使用两种不同表述：**三个旧建议未重放，旧 patch 应用 0 次；确认后的新修订已生效，新合并提案应用 1 次。** 后续提案 `01M226DVJZNTTSTGZ20CHGQDWD`、`01M226HF1GX6P579T58HV384G8`、`01M228VHNTEF3J3RXB6YPQYDNS` 以及本次 `ZS-DELIVER` 的流程写回只改变状态、摘要或下一动作，不含 `acceptance`，不构成第二次业务修订。

## 归档与责任移交的当前安排

| 内容类型目录 | 已确认维护角色 | 已确认来源 | 当前实施边界 |
| --- | --- | --- | --- |
| 使用说明 | 资料管理员 | `materials/archive-confirmation.md`、`materials/role-confirmation.md` | 目录名称和角色级职责已进入 acceptance；根路径、权限、实际建目录、资料迁移及完整性均未确认。 |
| 交接记录 | 值班人员 | `materials/archive-confirmation.md`、`materials/role-confirmation.md` | 目录名称和角色级职责已进入 acceptance；具体值班人员、值班表、代理安排、交接时限及实际归档均未确认。 |
| 问题跟踪 | 问题负责人 | `materials/archive-confirmation.md`、`materials/role-confirmation.md` | 目录名称和角色级职责已进入 acceptance；具体负责人、问题清单、影响、下一步、完成条件及升级路径均未确认。 |

协调者确认了角色级分工，但现有来源没有给出协调者、值班人员、资料管理员或问题负责人的具体姓名。不得把角色名称外推为人员已经接岗，也不得把验收文本外推为归档实施完成。

## 原始版本、失败与未持久化缺口

### 原始版本保持不变

- `materials/handover-draft.md`：SHA-256 `4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361`。
- `materials/change-requests.csv`：SHA-256 `0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2`。
- `materials/archive-confirmation.md`：SHA-256 `03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c`。
- `materials/role-confirmation.md`：SHA-256 `8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5`。
- 初次独立复核原文保持 SHA-256 `c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5`；独立返检报告 SHA-256 为 `5f8d595f33ce5fd48503d12e1076aab409b6f54605d36351ddf343e457d50173`。

### 失败历史继续保留

- 原生产链失败入口：`.work-receipts/cr-01-create-session-mismatch-error.json`、`.work-receipts/zs-recovery-final-doctor.log`。后者实际是 project revision 48 的中期非零 Doctor，不能按文件名解释为最终成功。
- 初次独立复核失败入口：`.work-receipts/reviewer-tool-failures.json`。
- 整改失败入口：`.work-receipts/zs-remediation-tool-failures.json`。
- 独立返检失败入口：`.work-receipts/reviewer-recheck-tool-failures.json`、`.work-receipts/reviewer-recheck-post-completion-tool-failures.json`，以及它们引用的原始 traceback、断言失败、缺参、证据契约失败、版本错误和两个零字节失败占位文件。
- 本次最终交付首次证据登记因输入多带不支持的顶层 `version` 字段被 AWR 以 `InvalidInput` 拒绝；project revision 保持 224，未产生证据或来源写入。原失败回执 `.work-receipts/zs-deliver-evidence-add-failure.log`、原草稿和原绑定报告继续保留，修订后使用 `v2` 文件，不覆盖原记录。
- 本交付没有删除、覆盖或改名上述历史；成功重试使用新的有效回执，不将失败文件改写成成功记录。

### 无法恢复的未持久化信息

旧中断 session 没有 checkpoint，因此无法确认或恢复中断前未落盘的思考过程、摘要、下一动作和开放循环。当前记录只继承有文件哈希、AWR 对象或事件回执支持的事实；此缺口永久保留，不以事后叙述补造。

## 已完成恢复工作与后续待办

| 分类 | 当前结论 |
| --- | --- |
| 已完成 | 区分首次正常退出与实际 `SIGKILL/-9`；固定信号前后来源和产物；中断旧 session、释放旧 claim 并由新任务恢复；保存和处置三个旧提案；完成初次独立复核指出的整改；将 CR-01/CR-02 确认要求一次写入权威 acceptance；重新完成恢复工作并通过不同参与者返检；形成本最终交接。 |
| 后续待办：具体问题事实 | 由有权执行者提供 CR-03 的具体未结束问题清单，并逐项给出已知影响、具体负责人、下一步和完成条件；若确实没有遗留问题，也须保存明确的“无”确认来源。当前不能指定实际人员。 |
| 后续待办：归档实施 | 确认目录根路径、访问权限、实际建目录结果、现有资料迁移范围和迁移后完整性校验。当前只有目录分类要求，没有实施证据。 |
| 后续待办：人员与升级 | 确认具体人员名单、值班表、代理/替补安排、升级路径、触发条件和交接时限。当前只有角色级职责，没有排班或升级实施证据。 |
| 后续待办：Git/发布/E4 | 由获授权流程决定是否将工作树形成提交、推送或发布，并另行验证；本次本地恢复交付不授予这些状态，也不计整场 E4。 |

以上待办不否定本次“中断恢复交接”已经完成；它们是因业务事实或实施证据仍未到位而明确留给后续工作的开放事项，不能计入已完成事实。

## 可追溯回执索引

- 中断与恢复：`review-inputs/interruption-evidence/actual-operator-interruption-v1.json`、`verified-interruption-binding.json`、`actual-recovery-binding.json`。
- 旧提案处置：`.work-receipts/cr-01-after-disposition.json`、`cr-02-after-disposition.json`、`cr-03-after-disposition.json`；库存核对为 `.work-receipts/zs-recover-proposal-inventory.json`。
- 新修订一次生效：`.work-receipts/zs-remediation-evidence-report.json`、`.work-receipts/zs-remediation-operation-index.json`。
- 初次复核与独立返检：`deliverables/zs-independent-review.md`、`deliverables/zs-independent-re-review.md`、`.work-receipts/reviewer-recheck-completion-report-v3.json`、`.work-receipts/reviewer-recheck-work-complete-v3.json`、`.work-receipts/reviewer-recheck-final-verification.json`。
- 本次最终交付：`.work-receipts/zs-deliver-session-start.json`、`zs-deliver-bootstrap.json`、`zs-deliver-context.json`、`zs-deliver-progress.json`、`zs-deliver-handoff-progress.json`、首次失败的 `zs-deliver-evidence-add-failure.log` 及 `zs-deliver-tool-failures.json`、有效的 `zs-deliver-verification-v2.json`、`zs-deliver-evidence-add-v2.json`、`zs-deliver-work-complete.json`、`zs-deliver-checkpoint.json`、`zs-deliver-session-end.json`、`zs-deliver-final-status.json`、`zs-deliver-final-doctor.json`、`zs-deliver-final-files.sha256`。

最终 AWR revision、完成事件、证据 ID、checkpoint 和 session 关闭状态以这些完成后落盘的 JSON 回执为准；本文件不预造动态 ID。任何后续对具体问题、归档实施、人员或升级安排的补充，都应作为新来源和新工作记录追加，不回写覆盖本次中断与失败历史。
