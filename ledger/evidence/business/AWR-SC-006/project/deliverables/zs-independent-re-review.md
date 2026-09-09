# 竹声中断恢复整改独立返检记录

> 返检结论：**整改范围通过（`remediation_recheck_passed`）**。此前 `ZS-R04`、`ZS-R06` 与 `ZS-F01` 至 `ZS-F03` 的阻断项均已由实际来源与 AWR 回执关闭。本结论只覆盖中断恢复整改和交接一致性，不是最终交付，不证明 Git 提交、发布或整场 E4。

## 复核身份与范围

- 实际返检者：`luna_worker`，沿用原独立复核角色；未参与生产者整改。
- 本次 AWR session：`01M227QGYJ3BWHQBQWTX6GBAMW`；claim：`01M227QGYK1C8M9HMNMWBCWH23`；工作项：`ZS-REVIEW`。
- 整改生产客户端：Codex native task `01a083d7-4073-7150-89cd-993a9899dcad`。操作方固定包为 `review-inputs/corrections/review-correction/`；`control/review-correction-publication.json` 登记 167 个 SHA-256。
- 返检只读取项目当前权威来源、生产者产物、固定整改包、公开中断证据和已登记回执。生产者自检包用于定位和交叉核对，不作为独立通过结论。
- 原独立复核 `deliverables/zs-independent-review.md` 保持原文，SHA-256 为 `c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5`。

## 固定输入与当前状态

1. 独立校验器逐项验证整改发布包的 167 个文件，文件集合与 `files_sha256` 完全一致；同时核对 v2 两份 SHA 清单、当前根来源与固定包来源、当前生产者产物与固定包产物。合计 247 项通过，见 `.work-receipts/reviewer-recheck-verification.json`。
2. 返检认领前，AWR 为 project revision 181、source revision 17，`ZS-RECOVER=completed`，`ZS-REVIEW=planned/ready`，没有活动会话；`ZS-DELIVER` 因 `ZS-REVIEW` 未完成而依赖阻断。
3. 返检前权威台账 SHA-256 为 `910155627fdf6131e7aed88c5297cc9c61a97c60aa4beec9a7c08475de8e4a37`。`ZS-RECOVER.acceptance` 精确为四条：原两条要求、已确认的归档要求、已确认的角色要求，没有重复项。
4. 生产者四份当前产物哈希与固定整改包一致：
   - `zhusheng-interruption-diagnosis.md`：`a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196`
   - `zhusheng-change-proposals.md`：`e00e5e4e98db2476d0321fc86af82e93aa0a218edfab66c1b568f64335304a05`
   - `zhusheng-recovery-work.md`：`46bfc0ea1ac8396b10280e06b3ed55a72a69242555010218974f8b4ece64fb31`
   - `zhusheng-consistency.md`：`ddb10a828a04059aea355186b98a685167ac738aee77902d587b6506b585260e`

## 逐项返检结论

| 编号 | 判定 | 实际依据 |
| --- | --- | --- |
| `ZS-R04` | **通过** | 新合并提案 `01M225AVE1BFYY0W4T2EKRV88C` 的 patch 精确保留原两条 acceptance 并加入两条已确认要求；提案状态为 `applied`，唯一 apply attempt 为 `01M225BCXTY18AWZSRG0B9WT1P`，resolved event 为 `01M225BCYBY571X8FPBY8TGME8`。写前/写后指纹分别为 `sha256:6ac162…12a` 与 `sha256:83f9e1…7a0`。当前四条 acceptance 与该 patch 完全一致。 |
| `ZS-R06` | **通过** | `ZS-RECOVER` 当前为 `completed`，其 summary 说明整改已提交返检，next action 明确等待不同参与者返检；不再要求生产者继续创建或应用业务变更。认领前 `ZS-REVIEW` 是唯一 ready 工作，`ZS-DELIVER` 仍受依赖阻断，状态和下一步一致。 |
| `ZS-F01` | **关闭** | 三份旧提案 `01M21X4H7GSP7DY19MTHYDBXST`、`01M21X5MHW7RFN8PBE4J86GACM`、`01M21X67QFPQ55YSMJEX96HYV7` 在库存中各出现一次，均为 `rejected` 且 `apply_attempt: null`；它们应用零次。确认后的 CR-01/CR-02 由上述新合并提案应用一次，归档和角色要求已进入当前权威 acceptance。 |
| `ZS-F02` | **关闭** | 恢复工作已完成的事实、等待独立返检的当前动作和后续交付依赖关系相互一致。生产者没有用完成状态掩盖仍需执行的业务修订。 |
| `ZS-F03` | **关闭** | 当前提案说明、一致性说明和恢复记录明确区分“旧 patch 应用 0 次”与“新合并提案应用 1 次”。之后提案 `01M226DVJZNTTSTGZ20CHGQDWD` 只修改 `summary`、`next_action`，`01M226HF1GX6P579T58HV384G8` 只修改 `summary`；两者 patch 均不含 `acceptance`，未造成第二次业务修订应用。 |

## 必须继续保留的边界

- CR-03 仍未获得具体问题清单。具体问题、影响、负责人、下一步和完成条件均不能补造；若实际没有遗留问题，也仍需执行者提供明确来源。
- 归档目录根路径、访问权限、资料迁移与完整性仍未知；具体人员、值班表、代理或升级路径、交接时限仍未知。当前 acceptance 只确认目录名称与角色级职责，不证明归档实施或排班已完成。
- 首次 prelude 的 native return code 为 `0`，继续按正常退出处理。之后实际 `SIGKILL` 与 native `-9` 的中断证据保留；自动观察器未触发中断的限制不得改写。
- 旧 session 没有 checkpoint。只能恢复已落盘文件、提案和事件，不能追认中断前未持久化的摘要、下一动作或开放循环。
- 原始跨工作项 session 失败、原复核工具错误、恢复中期非零 Doctor 和整改过程失败均保持原样。`zs-recovery-final-doctor.log` 仍只是 project revision 48 的中期非零记录，不能改称最终成功回执。
- 当前工作树仍未形成交付提交，`deliverables/zs-delivery.md` 不存在。返检通过后只能把 `ZS-DELIVER` 交回生产链处理。

## 本次返检真实失败

1. 独立校验器首次把 `work show` 的顶层 `acceptance` 误按为嵌套字段，产生 `KeyError: 'acceptance'`；原始 traceback 保存在 `.work-receipts/reviewer-recheck-verification-first-failure.log`，没有业务或 AWR 写入。
2. 第二次校验把跨两份生产者文档的措辞只在一致性说明内匹配，触发只读断言失败；原始输出保存在 `.work-receipts/reviewer-recheck-verification-error-2.log`。扩大到两份被审文档后，第三次运行 247 项全部通过。
3. 首次 `work progress` 调用遗漏必填 `--reason`，AWR 返回 exit 2，未改变 project revision；补齐参数后在同一 revision 正常应用。该失败纳入 `.work-receipts/reviewer-recheck-tool-failures.json`。
4. 首次 `work complete` 使用的完成报告缺少契约要求的顶层 `source_sha`，AWR 返回 `EvidenceMissing`，project revision 保持 190。原证据和失败报告保留；随后以新文件和新外部证据键登记完整 v2 证据，不覆盖失败历史。
5. 第二次 `work complete` 的 completion input 将契约版本误写为 2，AWR 返回 `InvalidInput` 并保持 project revision 191。最终 v3 证据仍使用新的外部键，但 completion report 与 input 均恢复为契约要求的 version 1。

## 交接结论

本返检确认生产者已完成此前要求的中断恢复整改，且未删除旧提案、失败或原复核结论。`ZS-REVIEW` 可据本报告和独立校验回执完成；完成后只解除 `ZS-DELIVER` 的依赖门槛。最终交付、Git 提交或发布、以及整场 E4 仍须由后续生产链以新证据处理。
