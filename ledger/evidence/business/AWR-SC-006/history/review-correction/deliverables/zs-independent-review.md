# 竹声维护交接独立复核记录

> 复核结论：**需原生产者整改（`requires_executor_changes`）**。本记录只完成独立复核，不是最终交付，不授予 E4，也不替原生产者修改交接产物或权威业务内容。

## 复核身份与范围

- 实际复核者：`luna_worker`，与生产客户端不同。
- 本次 AWR session：`01M223JC0DGNZACGCB1GK9FWQR`；claim：`01M223JC0DMZEZAF591R4QH2BC`；工作项：`ZS-REVIEW`。
- 中断前生产客户端：Codex native task `01a083b6-aa5f-7ba0-9135-c128ada421a2`。
- 恢复及两轮追问生产客户端：Codex native task `01a083d7-4073-7150-89cd-993a9899dcad`。
- 复核范围限于当前项目根来源、生产者产物与回执、`review-inputs/` 固定历史包及公开的中断取证。未读取私有原生全量上下文。
- 证据等级仅为 `locally_verified`。Git 工作树仍未形成正式交付提交，`deliverables/zs-delivery.md` 尚不存在。

## 复核依据与完整性

1. 操作方发布的 56 个历史轮次文件与 6 个中断取证文件全部逐文件通过 SHA-256 校验，结果见 `.work-receipts/reviewer-review-inputs-verification.json`。四个业务轮次的来源和产物哈希也与各自 `turn-record.json` 一致。
2. 复核前生产者的八个当前材料和产物已固定在 `.work-receipts/reviewer-producer-files-before.sha256`。其中最终四份生产者产物哈希为：
   - `zhusheng-interruption-diagnosis.md`：`a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196`
   - `zhusheng-change-proposals.md`：`576da8467d953ae91a4652460410375b53becdacf46507480b14351987b5ba39`
   - `zhusheng-recovery-work.md`：`365af74d286ae37c080b689f8f54904ad98468a5051b9939ae544567010cab03`
   - `zhusheng-consistency.md`：`a4b0e68ba92833cade1877458a7e029fd1b9ecca9035080d9d0358b096ca19fa`
3. 生产者最终来源台账哈希为 `18f18e0fe31ae9c1a5951f6aad176be41cba613c0fd7a6d0fa65435b7395e846`。本复核之后只允许由 AWR 更新 `ZS-REVIEW` 的运行状态和证据；生产者工作项与产物保持只读。
4. 生产者提供的 16 项复核输入清单再次执行通过，输出见 `.work-receipts/reviewer-producer-input-verification.log`。该检查只证明文件和提案状态符合脚本断言，不替代本次业务判断。

## 中断与恢复链判断

- 首次 prelude 的 native return code 为 `0`，是正常退出，不能计为中断。
- 后续自然业务提案请求在三个实际提案进入 `ready` 后受到一次实际 `SIGKILL`；native return code 为 `-9`。自动观察器没有执行故障注入，这个失败由 `actual-operator-interruption-v1.json` 和 `verified-interruption-binding.json` 单独绑定，不能抹去或改写。
- 中断时三个提案、两份产物及四个权威来源哈希在信号前后保持一致。新 native task 随后实际恢复 AWR 工作；事件历史含 project revision 46 的 `session.resumed` 与 `session.resumed_from`。
- 旧 AWR session `01M21WZXPSGZ77369CSGXCB8CX` 当前为 `interrupted`，旧 claim 已释放；接续 session `01M21XP8VZ1YJRNSS6BJH2V4HB` 已正常结束并有后续 checkpoint。
- 旧 session 的 `last_checkpoint_id` 为 `null`。因此只可确认已经落盘的文件、提案和事件；中断前未持久化的摘要、下一动作或开放循环不能恢复，也不能补造。

## 逐项复核结论

| 编号 | 检查项 | 判定 | 依据与影响 |
| --- | --- | --- | --- |
| ZS-R01 | 真实中断与不同 native task 恢复 | 通过 | 首次 prelude 正常退出与后续 `SIGKILL/-9` 已区分；恢复事件、旧 session 中断和 claim 释放均有实际回执。 |
| ZS-R02 | 已保存结果、来源与历史包完整性 | 通过，带边界 | 62 个发布文件哈希全部匹配，当前生产者产物与 round-2 固定结果一致。无旧 checkpoint，因此不能证明未落盘的主观摘要未丢失。 |
| ZS-R03 | 防重复与旧提案处置 | 通过，限于“没有重放” | 三个旧 ID 各只有一条库存记录，均为 `rejected`，`apply_attempt: null`，immutable patch、失败和拒绝回执均保留。此证据表示业务 patch **应用零次**，不能表述为“只应用一次”。 |
| ZS-R04 | 已获确认的业务修订是否实际生效一次 | **不通过，阻断** | round-1 的归档确认和 round-2 的角色确认已经到位，但 CR-01/CR-02 对应的确认后修订仍未进入当前权威来源；三个旧提案全部拒绝，且没有新的合并业务提案及一次成功应用回执。后续两个 `apply_started` 属于 `work.progress`/`work.complete` 流程状态写回，不能替代业务 acceptance 修订。 |
| ZS-R05 | 未知项与交付边界 | 通过 | 目录根路径、权限、迁移与完整性，具体人员/值班与升级路径，以及具体遗留问题、影响、负责人、下一步和完成条件均继续标为未知；未预填独立复核或最终交付。 |
| ZS-R06 | 当前权威状态与剩余工作一致 | **不通过，阻断** | `ZS-RECOVER` 已标为 `completed`，但同一条目的 `next_action` 仍要求取得具体遗留问题、提交并应用合并变更，再核对来源和回执。完成状态与明确剩余动作不一致。 |

## 阻断缺陷与处理要求

### ZS-F01：确认后的 CR-01/CR-02 没有一次性生效证据

`materials/archive-confirmation.md` 已确认三个目录名称，`materials/role-confirmation.md` 已确认角色级职责边界。生产者的接续 checkpoint 明确要求基于最新来源合并去重，但最终处置只拒绝了三份旧覆盖式提案，没有创建或应用替代提案。当前 `ZS-RECOVER.acceptance` 仍只有原始两条。

原生产者需要基于届时最新来源完成下列处理：

1. 把已经获得业务确认的 CR-01 与 CR-02 修订合并为单一、可审阅的当前来源变更，并保留各自来源引用。
2. 只执行一次经审阅的应用，保存明确的 `apply_attempt`、写前/写后指纹和最终来源哈希；不得重放三个旧 patch。
3. CR-03 继续保留为未决。具体遗留问题清单没有到位时，不得把它并入已确认事实；若执行者明确确认“无遗留问题”，也须保存该业务来源。
4. 若业务负责人决定确认内容只应进入交接说明而不进入权威台账，需要新增明确的业务授权来源并据此修订原来的“合并处理替代”说法；现有生产者自述不能代替授权。

### ZS-F02：`ZS-RECOVER` 的完成状态与下一动作冲突

原生产者应通过正常 AWR 流程恢复一致状态：在 ZS-F01 和具体遗留问题边界处理完毕前保持可继续整改的状态；整改完成后再以新的证据完成。不能保留“仍需提交并应用”的 `next_action` 同时宣称该工作已经完成。

### ZS-F03：完成表述必须区分零次业务应用与流程状态写回

当前材料对“旧提案均无 apply attempt”记录准确，但 `ZS-RECOVER` 的完成证据将“零次应用”作为恢复已完成的重要依据。整改材料应明确：

- 三个旧 patch 的零次应用证明没有重放；
- CR-01/CR-02 的确认事实是否生效，要由新的业务变更回执单独证明；
- `work.progress` 和 `work.complete` 的来源写回只是工作流程状态变化。

## 必须保留的真实失败与限制

- `.work-receipts/cr-01-create-session-mismatch-error.json`：首次跨工作项 session 绑定创建被拒绝，exit code `1`；没有创建提案或修改来源。
- 自动观察器没有执行计划中的中断；实际中断由操作方一次 `SIGKILL` 和 native `-9` 单独证明。
- `.work-receipts/zs-recovery-final-doctor.log` 实际记录的是恢复中期 project revision 48 的非零 Doctor：数据库完整，但存在活动 session 和三个有意保留的 pending mutation。该文件名不能被当成最终 Doctor 成功回执；round-2 的最终成功说法仍受其实际时点约束。
- 旧 session 没有 checkpoint，未持久化信息不可追认。

## 返检与交付门槛

原生产者整改后，应提交新的固定整改快照供同一独立复核者逐条返检 ZS-F01 至 ZS-F03，并核对生产者文件前后哈希。返检通过只确认本次恢复交接范围；最终 `zs-delivery.md`、Git 提交/发布和 E4 仍须由原生产链按后续门槛处理。
