# 望海运营手册独立复核记录

- 复核日期：2026-09-09
- 复核工作项：`WH-REVIEW`
- 独立复核者：`/root/restricted_material_client_run`（实际 `luna_worker` 任务）
- AWR 会话：`01M223E7S897SM2ADAAQ1AX25Q`
- AWR claim：`01M223E7S879SQHTJYJ5HAKFEF`
- 生产者客户端：旧前序 `01a08392-3f62-74a0-b844-087b9ef0b1c1`；恢复后 `01a083b2-ae71-77a3-9cc9-977b6c656d5a`
- 复核结论：**业务复核范围有条件通过；既有客户端范围偏离和工具失败须继续披露，不能据此宣称完整过程合规、正式批准、实际最终交付或 E4 完成。**

## 1. 复核范围与证据

本次直接核对当前权威来源、生产者产物、AWR 回执、五个真实业务轮次的封存摘要，以及 `review-inputs/` 的固定包。固定包发布清单 SHA-256 为 `5462dcaecee99d4271f547e9bf6fad180de20d61d77379b9866d2dedfab4409a`；清单声明的 101 个文件全部存在且逐个哈希一致，无缺失、无多余文件、无哈希漂移。独立复核验证回执为 [`.work-receipts/reviewer-package-verification.json`](../.work-receipts/reviewer-package-verification.json)。

复核前 20 份生产者产物均与 `round-2/turn-record.json` 的封存哈希一致。关键当前对象如下：

| 对象 | SHA-256 | 复核用途 |
| --- | --- | --- |
| [`wanghai-before-handoff.md`](wanghai-before-handoff.md) | `f4c58c9422f8452f3c0bd148859be94d2ecceebe670fe7c62c21401292773efc` | 旧客户端形成的正式暂停交接 |
| [`wanghai-operations-manual-draft.md`](wanghai-operations-manual-draft.md) | `813bd164add4ed663223d083e56b76a9f57a2c372906236bcad713712c2b0545` | initial 版工作稿 |
| [`wanghai-operations-manual-draft-v2.md`](wanghai-operations-manual-draft-v2.md) | `16011e71b8d4bb2b0f588c93b325f07491b3d33bea4052c89ad6c751bbe65438` | round-1 新材料修订 |
| [`wanghai-operations-manual-draft-v3.md`](wanghai-operations-manual-draft-v3.md) | `b09950015623422d6ae287dd6295abbd83b6ea47970f7da64835d3957f222e68` | round-2 新验收规则修订 |
| [`wanghai-progress-handoff.md`](wanghai-progress-handoff.md) | `b4b606c662296ebd99c4d695395f4a0d26f73a773bc5c848749324945c70add6` | initial 推进交接 |
| [`wanghai-progress-handoff-v2.md`](wanghai-progress-handoff-v2.md) | `9ed1028c36b4fa525424f79bb17aa61157384582961dd34ad7743ee9cd93d341` | round-1 推进交接 |
| [`wanghai-progress-handoff-v3.md`](wanghai-progress-handoff-v3.md) | `526fe9513c657c927ff71a6d887f353508612ab0b29b2afe32ba7f8d2feb3bff` | round-2 验收调整交接 |
| [`wanghai-source-verification-v3.md`](wanghai-source-verification-v3.md) | `267dae9476a283a3bdaeb2539a9912e1849294e22db5bb16d300ae3f5459dbba` | 当前来源核对 |
| [`wanghai-final-delivery.md`](wanghai-final-delivery.md) | `ad31de837d2d7be3d2cb22fb6cc8acf2de55b3ddc7ae4cb1a1c5c4765bec48f0` | 待复核交付候选，不是实际最终交付 |

## 2. 中断、交接与恢复链

| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| 前序业务与正式暂停 | 通过 | `prelude` 和 `prelude-handoff` 的封存 turn 均绑定旧客户端；正式暂停输入、最终回复和 `wanghai-before-handoff.md` 哈希一致。 |
| 作者证据 | 通过 | `prelude-handoff/turn-record.json` 明确记录旧客户端及该产物哈希；本结论没有把文件进入快照误当作作者证据。 |
| AWR handoff | 通过 | 旧会话 `01M21TD468G8SZDJTQ2WT287TA` 以 `incomplete` 结束，检查点 `01M21TKFJDEA7YW3E0E4ST7ASA` 保存六个 open loops，claim 已释放。 |
| 新客户端恢复 | 通过 | `actual-resume-binding.json`、`WH-DRAFT-session-resume-r36.json` 和真实 initial turn 一致绑定新客户端；从旧会话恢复到 `01M21VAYWH5J13AEPREX0HGE8R`，六个 open loops 无丢失。 |
| 后续连续性 | 通过 | initial、round-1、round-2 均绑定恢复后的同一客户端；两个真实追问分别引入 `duty-notes.md` 和 `RULES.md` 新要求，并形成独立 v2、v3 文件。 |

恢复绑定回执的发布哈希与实际文件均为 `63b6682006e79cab1fa9bb85948189fba752654f45d6788d41f89e3adc7f5873`。因此恢复链有真实会话、检查点、open loops 和新旧客户端身份共同支撑。

## 3. 三轮产物与来源保持

### 3.1 initial

initial 依据原始 `manual-outline.md`、`open-questions.csv` 和前序交接形成工作稿、来源核对和推进交接。它把内容限制在三章结构、七项缺口和编辑模板，没有把缺失草稿、早班记录、FAQ 或升级规则补造成事实。

### 3.2 round-1

新增 `duty-notes.md` 只支持三项事实：当班协调者判断异常是否影响下一班、资料管理员补充操作说明入口、晚班交代未完成事项的影响范围。v2 将 `GAP-01/03/04` 仅标为“部分补充”，继续把具体人员、链接、完整规则和批准状态列为未知；`GAP-02/05/06/07` 保持未补充。新旧文件哈希均保持，未覆盖 initial 证据。

### 3.3 round-2

当前 `RULES.md` 新增逐章出处、未确认内容的影响与负责角色、最终说明内材料核对记录三项要求。v3 的新增内容集中在验收结构、责任影响展开和交付候选说明，没有新增联系人、时限、FAQ 正文、链接或操作规则。v1、v2、v3 的工作稿、来源核对和推进交接均保留为不同文件，哈希与各轮固定快照一致。

## 4. 当前候选逐项判定

| 检查项 | 结论 | 说明 |
| --- | --- | --- |
| 六个二级章节逐章出处 | 通过 | 六章标题后的首个非空内容均为“本章资料出处”；引用文件存在，所列行号支持相邻限定性陈述。 |
| 每日启动检查责任影响矩阵 | 通过 | 4 个未确认项均含影响、负责角色及当前状态、恢复条件。 |
| 值班交接责任影响矩阵 | 通过 | 6 个未确认项均含影响、负责角色及当前状态、恢复条件。 |
| 常见问题处理责任影响矩阵 | 通过 | 5 个未确认项均含影响、负责角色及当前状态、恢复条件。 |
| 总缺口状态 | 通过 | `GAP-01/03/04` 为部分补充，`GAP-02/05/06/07` 未补充；七项均未完全关闭。 |
| 本地交叉引用 | 通过 | 四份当前候选文档共核验 50 个本地链接，均可解析到现有文件。 |
| 最终说明中的材料核对 | 通过 | 第 2 节含日期、执行者、范围、维度、对象级结果、哈希、缺口和边界；五个列示哈希均与当前文件一致。 |
| 候选与实际最终交付边界 | 通过 | 标题和正文均明确“待独立复核/交付候选”；实际最终文件 `wh-delivery.md` 仍是后续产物。文件名中的 `final-delivery` 不构成实际最终交付。 |
| 真实运营和批准状态 | 未发生，符合披露 | 当前材料不支持真实运营演练、业务批准、生效版本或 E4；候选没有提前宣称完成。 |

## 5. 保留的过程偏离与真实失败

### P-01：客户端检索范围偏离

项目 `AGENTS.md` 只允许项目目录及列明的公共技术文档。真实轨迹仍保留以下偏离：

1. `prelude` `item_2` 尝试检索 `/Users/mac/.codex/memories/MEMORY.md`，command SHA-256 `ddf36e7cc9b107bac331f90623476711ad017db5844c7fad457fd50094d7aa12`，退出码 1；输出只有允许的 `RTK.md` 内容，没有 memory 命中。
2. `initial` `item_2` 再次检索该 memory 文件，command SHA-256 `324cd119e2c7d8639fb4b567df7f31adef666510b5d5fc1da8d4116619bfd029`，退出码 1，输出为空。
3. `round-2` `item_3` 再次检索该 memory 文件，command SHA-256 `355f1d7e7bfd84aab16925cbc351f0e9432ee6886b1b19f287b9cc096ff15d28`，退出码 1，输出为空。
4. `prelude` `item_62` 把父级整个 `docs/` 作为技术检索范围，超出列明路径；command SHA-256 `743d3f809f160cbd89772977cba74147d92dda73171371004d5434d401dbf60a`。实际输出只有允许的父级 `README.md:380`，没有返回其他文档内容。

判定：这些是实际过程范围偏离，不能追认为合规。由于三次 memory 检索无命中、广域 docs 检索仅返回允许文档的一行，当前证据没有显示外部业务答案进入手册；这不消除偏离本身。原生产者在实际最终交付中必须保留此披露，不能宣称执行过程完全符合读取边界；场景级是否接受该偏离由后续验收决定。

### P-02：工具失败

五个真实阶段共保留 6 个非零命令：上述三次无命中的范围偏离，以及以下三次 AWR 写入失败：

- initial `item_81`：`WH-DRAFT` 首次完成因 evidence report 缺少单数 `command` 字段被拒；随后使用 v2 验证报告成功完成，成功回执为 `WH-DRAFT-complete-r54.json`。
- round-1 `item_24`：`work reopen WH-LINKS` 因该动作不允许 `summary` 字段被拒；去除该字段后成功，回执为 `WH-LINKS-reopen-r84.json`。
- round-2 `item_124`：创建 `WH-REVIEW` next-action proposal 时因目标工作项与创建会话冲突被拒；随后以不绑定该生产会话的 proposal 路径成功应用，事件回执为 `WH-REVIEW-next-action-apply-event-r196.json`。

上述失败均被保留，没有清理或改写；成功重试与当前 AWR 状态一致。

## 6. 未决事项与退回要求

业务内容本身没有发现需改写 v3 的阻断缺陷，因此不要求独立复核者或生产者回写现有候选。以下事项必须交回原生产者和操作方处理：

1. 原生产者形成实际 `wh-delivery.md` 时，继续保留七项缺口及其影响、负责角色和恢复条件；不得把本复核提升为业务批准、真实运营验收或生效证明。
2. 原生产者在实际最终交付中披露 P-01、P-02，不得描述为无范围偏离、无失败的完整合规过程。
3. `wanghai-final-delivery.md` 保持历史候选，不得把它或本复核文件直接改称实际最终交付。
4. run 根 `execution-record.json` SHA-256 为 `2b916f25b53c18a2223ee0d7fff080ed3132cd1d10b449c623d5eed04873b2bd`，仍含“客户端和独立复核待授权/未绑定”的旧阻塞文本，且 `independent_review`、`final_delivery`、各 gate 仍为空。它与已封存客户端执行及当前独立复核状态不一致；这是操作方记录待核点，不能由生产者或本复核者改写，也不能单独作为最终状态依据。

## 7. 结论边界

本复核确认：恢复链、三轮来源演进、六章出处、三个责任影响矩阵、材料核对记录和候选边界满足当前 `WH-REVIEW` 的业务检查范围。原生产者可以据此处理披露要求并进入实际最终交付工作。

本结论不批准运营手册投入使用，不确认缺失业务事实，不证明真实运营演练、正式发布、远端落地或 E4 完成，也不消除 P-01 的过程偏离。实际最终交付必须由原生产者在本复核完成后另行形成。
