# 云帆帮助中心独立复核报告

## 复核结论

- 复核日期：2026-09-09
- 复核者：`luna_worker`
- AWR 工作项：`YF-REVIEW`
- AWR 会话：`01M224VSS7GA3E7T9VF92TPQB3`
- Git 来源基线：`ed1368bf06806ff3227af899129d4914d5fe81e0`
- 判定：**当前 process-correction 版本通过独立复核，无需退回生产者修订；本结论只支持完成 YF-REVIEW，不构成最终交付或整场 E4。**

生产者当前三份主产物与已发布的 `process-correction` 快照逐字节一致。现行结论准确区分了三条项目输入、六个 Agent 自拟检查问题、零次使用者针对这三条请求的后续追问、三条说明修订链、零次源材料修改和零次真实系统操作。旧阶段中的范围误判、输入到达时间误报和对模拟检查问题的错误归因均继续保留为历史证据，没有被覆盖或追认为成功。

`YF-USE` 的范围是当前 Agent 客户端使用项目文档形成可执行说明并保存返工记录。按这一合同，三条输入均已被处理，三份说明和修订链存在，因此执行者侧的两条 YF-USE 验收可以成立。该证据不证明真实账号申请、CSV 导入或问题反馈在外部系统执行成功，也不证明客户 UAT 或使用者参与多轮对话。

## 审阅输入与完整性

- `control/review-input-publication.json` SHA-256：`00799f21c321fb6029d0492e3cc3a18736203e2cff08134a3e146e134626e7af`。发布单绑定 `94` 个文件：五个历史阶段共 `93` 个文件，加 `review-inputs/README.md`；逐项重算，缺失 `0`、不匹配 `0`。
- 五个阶段为 `initial`、`usage-scope-clarification`、`round-1`、`round-2`、`process-correction`。每个 `turn-record.json` 声明的来源和产物哈希均与对应快照一致，共核对 `75` 个声明哈希，错误 `0`。
- `control/usage-process-review-publication.json` SHA-256：`1df92147c7f11077709abe8faf6d354ee187bdeff9ac94493600a5a843c2027d`。其绑定的 `review-inputs/usage-process-observation.json` SHA-256 为 `29fa1f377f792204c13811480e515808563091f2b1f0691020d445f41e74093d`，重算一致。
- 独立机器核对结果保存在 `.work-receipts/reviewer-independent-check.json`，SHA-256：`c1fc38d4ee49f39929eb9ca76b351bc78163298338d8734cc340d69f0d75c27d`。

## 实际过程事实

| 事实 | 独立判断 | 依据 |
| --- | --- | --- |
| 项目输入 | `3` 条，分别为账号分工、CSV 导入说明、问题反馈材料 | `materials/usage-inputs.csv:2-4`；SHA-256 `b3fd3f7e16405a84fea59bbefb70d5ada50d7659a8cb7274d6f65be51897798f` |
| 输入到达顺序 | CSV 在 canonical round-1 用户输入前已发布；执行者在该轮稍后才识别到它 | `review-inputs/usage-process-observation.json` 记录发布于 `2026-09-09T09:24:27+08:00`，round-1 用户输入为 `2026-09-09T01:24:40.665Z` |
| Agent 检查问题 | `6` 个，每案两个，均由 Agent 在同一原生 round-1 业务轮次内构造 | `review-inputs/round-1/turn-record.json` 与 process-correction 使用报告 |
| 使用者针对三条请求的后续追问 | `0` | round-1 过程观察只有一条场景级自然用户输入；六个内嵌问题不是外部用户消息 |
| canonical 业务追问 | `2` 轮，即 round-1 与 round-2 | 两个阶段各自的 `turn-record.json`；它们是场景级返工请求，不是三条输入的使用者追问 |
| process-correction | 单独的事实纠正轮次 | `review-inputs/process-correction/turn-record.json` |
| 源材料修改 | `0` | `help-draft.md` 在各阶段哈希不变；CSV 自 round-1 起哈希不变，当前仍相同 |
| 说明文字修订 | `3` 条修订链 | 当前使用报告的 U-01、U-02、U-03 均保留初版说明、两个 Agent 检查问题、实际说明修订和最终说明 |
| 真实系统操作 | `0` | 三个案例均明确未提交、未导入、未反馈，且无外部系统回执 |

三条项目输入是验收材料中的真实收到请求，但没有客户现场来源证明。把它们称为“项目提供的验收输入”准确；把它们称为客户 UAT 输入或实际客户请求则不准确。

## 三份说明复核

| 说明 | 草案支持事实 | 修订后产物判断 | 仍未确认 |
| --- | --- | --- | --- |
| 账号申请 | 需要说明申请者和审核者职责 | 已形成职责清单、前置确认和回执检查，新增内容明确标为 Agent 建议 | 正式入口、字段、审核者映射、标准、时限、紧急规则 |
| CSV 导入 | 支持 CSV 与 JSON | 已形成有条件导入检查单，没有把格式支持扩写成系统成功 | 编码、schema、字段、限制、去重、回滚和成功判定 |
| 问题反馈 | 需要复现步骤和联系角色 | 已形成可复查反馈单，并保留去敏和角色确认边界 | 渠道、联系人、SLA、数据共享政策和正式关闭标准 |

三份说明均引用草案和输入，保留无法确认内容，并明确没有真实执行。因此它们满足“文档协助使用”的可执行说明要求，同时不能作为外部系统成功回执。

## 十条验收映射

生产者映射在 process-correction 冻结时共有 `10` 行：G-1/G-2、U-1/U-2、M-1/M-2 为执行者侧满足，R-1/R-2、D-1/D-2 为未满足，即 `6` 条满足、`4` 条未满足。十行均包含证据或明确缺失、执行方式、来源版本、证据层级和判断，映射结构与当前台账原文一致。

| 分组 | 复核结果 |
| --- | --- |
| G-1 / G-2 | 通过；当前缺口报告正确分层并保留来源、未知项及未执行边界。 |
| U-1 / U-2 | 通过；三条输入和三份说明修订链存在，但只证明当前 Agent 的文档协助使用。 |
| M-1 / M-2 | 通过；十条映射完整，旧结论和当前结论有版本区分。 |
| R-1 / R-2 | 本独立报告及其 AWR 证据登记完成后具备通过证据。 |
| D-1 / D-2 | 仍未满足；生产者尚未处理本复核、生成 `deliverables/yf-delivery.md` 或形成最终交付回执。 |

因此，生产者的 `6/4` 是独立复核发生前的正确冻结结论。本复核完成后，当前有效口径可更新为前八条已有证据、最后两条仍待 YF-DELIVER；生产者的冻结映射不应被反向改写。

## 历史不准确结论与失败保留

1. 初始阶段曾把外部帮助中心入口、环境和账号视为 YF-USE 前置条件，并阻塞 YF-USE；后续范围澄清已撤回这一误判。原 `work.blocked` 回执和 incomplete 会话继续保留。
2. round-1 和 round-2 使用报告曾写“随后工作区新增 CSV”，与发布时序不符；process-correction 已按用户澄清及过程观察改为“round-1 开始前已经提供”。
3. round-1 和 round-2 把六个 Agent 自拟检查问题写成“追问”及“两轮返工”，没有充分区分用户参与；process-correction 已明确为 `6` 个 Agent 检查问题、`0` 次使用者后续追问。
4. AWR 仍保留旧版 locally_verified 记录，包括执行者自拟代表性输入和旧范围判断。这些是历史记录；当前结论只采用 V2/V3 修订证据，不把旧记录重新标成当前成功证据。
5. AWR 历史中三个生产者会话状态为 `incomplete`：`01M21TEK3RJ1QV2YFW7K1QZWKQ`、`01M21V96JS4PYZS8K2JWYYCWXQ`、`01M21XAD13ZHWFB5A2NEDC080P`。它们的 checkpoint、open loops 和已释放 claims 均保留，本复核不把这些终态改写为 ended 或 completed。
6. 本复核首次独立检查因相对路径解析错误失败一次；三次历史 session 读取又因误用 `--session` 参数失败。失败分别固化在 `.work-receipts/reviewer-verification-failure.json` 和 `.work-receipts/reviewer-session-inspection-failures.json`，随后使用绝对路径和位置参数成功重跑；没有改动生产者文件或运行态。
7. AWR 首次完成尝试因 evidence locator 指向 Markdown 而被完成门拒绝；第二次尝试使用机器报告，但报告缺少顶层 `command` 字段，再次被拒绝。两次 `EvidenceMissing` 回执分别保存在 `.work-receipts/reviewer-complete.json` 和 `.work-receipts/reviewer-complete-success.json`；YF-REVIEW 未在失败尝试中变更为完成。

## 生产者修订链哈希

| 产物 | round-2 修订前 | process-correction 修订后 / 本复核前当前值 |
| --- | --- | --- |
| `deliverables/yunfan-evidence-gaps.md` | `08b47a96b29995466b106232b3289df8552a4e53d89a88e82437980d98ddd4da` | `60c9cdc0ad0ef3437c6666c94dbc9a5161c3bb0e370ef4b8a35d1a39899e6d14` |
| `deliverables/yunfan-usage-report.md` | `bfa30a29d86606a2723737f560c3c2751292130aab945324083e1b47d87af9c9` | `a8230b62891dbb0e2dc5eeafa302978692472ebb8fc7c04d2900de6d45420400` |
| `deliverables/yunfan-delivery-receipt.md` | `a372b92eff1a59854b9421613334d9c42412e5987b04a92e639f3fa750849c7c` | `c8afa5315ea6594ac2c9ec9ec4766008eaa5b8ca82f7e0f6997327df7ed8ea10` |

当前三份产物均与 process-correction 发布快照相同。生产者源码台账在 initial、范围澄清、round-1、round-2、process-correction 的哈希依次为 `9c652979...`、`db17d5e7...`、`76991c89...`、`45102089...`、`2c14c875...`；完整值保存在独立机器核对回执中。GOALS 和 PLAN 全程不变，RULES 在 round-2 前新增逐条映射规则后保持不变。

## 未决项

- 生产者需在 `YF-DELIVER` 中读取并处理本复核，形成 `deliverables/yf-delivery.md` 和可追溯最终回执。
- 本复核没有执行外部网站、账号、导入或反馈操作；也没有客户来源证据，因此不授予客户 UAT、真实系统验收或整场 E4。
- 在 YF-DELIVER 完成前，项目里程碑仍应保持进行中，最终交付仍为 No-Go。
