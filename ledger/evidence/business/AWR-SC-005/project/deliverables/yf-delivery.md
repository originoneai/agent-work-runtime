# 云帆帮助中心材料最终交付与责任移交

## 交付结论

- 交付日期：2026-09-09
- 最终工作项：`YF-DELIVER`
- 执行者会话：`01M226DZ32RQK3N9Z25423FNHR`
- 独立复核者：`luna_worker`
- 独立复核会话：`01M224VSS7GA3E7T9VF92TPQB3`
- Git 来源基线：`ed1368bf06806ff3227af899129d4914d5fe81e0`
- 最终判断：**按本项目定义的“当前 Agent 客户端依据帮助材料处理请求、形成可执行说明并保存可复查过程”的验收范围，当前材料包可以完成交付。**

独立复核确认 `process-correction` 版本通过，无需生产者返工。复核前冻结的十条映射是 `6` 条满足、`4` 条未满足；独立复核完成后，R-1、R-2 获得证据，状态成为 `8/2`；本文件、配套机器验证和 AWR `work.completed` 回执共同形成后，D-1、D-2 才具备证据，当前工作项口径更新为 `10/10`。

这里的“完成交付”只指本项目的帮助中心材料交付。它不表示真实账号申请、CSV 导入或问题反馈已经执行成功，也不表示客户 UAT、生产验收或整场 E4 通过。真实系统操作仍为 `0`，三条输入也没有客户现场来源证明。

本文件是最终交付产物；最终证据使用外部键 `YF-DELIVER-FINAL-MATERIAL-PACKAGE-20260909`，机器报告位于 `deliverables/yf-delivery-verification.json`。AWR 生成的证据 ID、完成事件 ID 和最终项目修订记录在 `.work-receipts/` 中，不回填本文件，以免破坏已经登记的内容哈希。

## 当前可用说明

完整说明及每一步的来源、Agent 建议、未知项和未执行边界保存在 `deliverables/yunfan-usage-report.md`。实际使用时采用以下受约束步骤：

### 账号申请

1. 申请者先写清业务用途、权限范围、使用期限和负责人；这些是 Agent 建议的最小准备项，不是草案规定的正式字段。
2. 向已确认的系统负责人、管理员或直属负责人询问正式入口、必填字段、对应审核者和审批规则。
3. 只有在入口与审核者得到确认后，才按正式流程提交并保存申请编号或审批回执。
4. 草案没有证明“先开通、后审批”被允许；未取得回执前不得描述为账号已申请或已开通。

尚未确认：正式入口、字段、审核者映射、审核标准、处理时限和紧急开通规则。真实申请、审批和账号创建均未执行。

### CSV 导入

1. 现有草案只确认支持 CSV 与 JSON，不能推定编码、分隔符、表头或字段结构。
2. 执行前向系统负责人取得当前模板或 schema，并确认编码、表头、必填字段、大小与记录数限制、去重、错误处理和回滚规则。
3. 只有在目标环境、权限和数据授权明确后，才使用去敏的最小样本，在指定的非生产或安全测试范围执行试导入，并记录文件哈希、时间、结果与错误信息。
4. 样本通过且回滚方式明确后，才可按获批方案扩大执行；成功与否以真实系统回执为准。

尚未确认：入口、编码、schema、字段、限制、去重、回滚和成功判定。没有读取客户文件，也没有上传、导入或取得导入回执。

### 问题反馈

1. 整理环境、时间、逐步复现步骤、预期结果、实际结果、影响范围和已尝试措施。
2. 附件只保留定位所需的最小信息并去敏；草案没有定义可共享数据范围。
3. 提交前确认正式反馈渠道、具体联系人、允许的附件与日志、响应时限和关闭标准。
4. 通过确认后的渠道提交，保存工单号或消息回执；以责任方确认处理结果和正式关闭条件作为闭环依据。

尚未确认：反馈入口、联系人、SLA、数据共享政策和正式关闭标准。没有提交反馈，也没有取得处理或关闭回执。

## 实际使用过程与纠正后的事实

| 过程事实 | 当前准确记录 | 证据 |
| --- | --- | --- |
| 实际收到并处理的请求 | `3` 条项目提供的验收输入；没有证据证明其来自客户现场 | `materials/usage-inputs.csv:2-4`；`deliverables/yunfan-usage-report.md` |
| 输入到达时间 | CSV 在 canonical round-1 用户输入前已提供；执行者是在该轮稍后才识别到文件 | `review-inputs/usage-process-observation.json`；`review-inputs/round-1/turn-record.json` |
| Agent 自拟检查问题 | `6` 个，每案两个，均由 Agent 在同一个原生 round-1 业务轮次中构造 | `deliverables/yunfan-usage-report.md`；独立复核报告 |
| 使用者针对三条请求的后续追问 | `0` | 过程观察与独立复核报告 |
| canonical 业务返工请求 | `2` 轮，即 round-1 和 round-2；它们是场景级请求，不是上述六个示例问题 | `review-inputs/round-1/turn-record.json`；`review-inputs/round-2/turn-record.json` |
| 事实纠正轮次 | `process-correction` 是单独一轮，用于纠正到达时间、角色归因和修订范围 | `review-inputs/process-correction/turn-record.json` |
| 源材料修改 | `0`；`materials/help-draft.md` 和 `materials/usage-inputs.csv` 未因使用过程修改 | 各阶段哈希链及独立复核报告 |
| 说明修订 | `3` 条；实际修改的是使用报告内的说明文字及受影响的执行者侧证据、映射和工作记录 | `deliverables/yunfan-usage-report.md` |
| 真实系统操作 | `0`；未申请账号、未导入文件、未提交反馈 | 使用报告、缺口报告、独立复核报告 |

因此，报告里的六个检查问题只能证明同一 Agent 对说明进行自检和文字返工，不能写成使用者参加了多轮对话。输入表“后来才到达”的旧说法也已撤回；准确说法是文件先到，执行者后发现。

## 当前材料与来源绑定

| 标识 | 当前产物或来源 | SHA-256 / AWR 证据 |
| --- | --- | --- |
| S-DRAFT | `materials/help-draft.md` | `bbb1980471ead2a84b1c641c0536ff81d547ec0beac7366db1439ad091520941` |
| S-INPUT | `materials/usage-inputs.csv` | `b3fd3f7e16405a84fea59bbefb70d5ada50d7659a8cb7274d6f65be51897798f` |
| A-GAPS | `deliverables/yunfan-evidence-gaps.md` | `60c9cdc0ad0ef3437c6666c94dbc9a5161c3bb0e370ef4b8a35d1a39899e6d14` |
| E-GAPS | `deliverables/yunfan-evidence-gaps-current-v3-verification.json` | `f8e25f0b5e8e7caec1a0af9fdb84f3f77e1fd51db8efd173dec9d0212fc34477`；`YF-GAPS-CURRENT-STATE-CORRECTION-V3-20260909` |
| A-USE | `deliverables/yunfan-usage-report.md` | `a8230b62891dbb0e2dc5eeafa302978692472ebb8fc7c04d2900de6d45420400` |
| E-USE | `deliverables/yunfan-provided-inputs-v2-verification.json` | `e259316cf0665826f10c8170995752495f567dfe4e0f25a5d4c6b97da5d65bd9`；`YF-USE-PROVIDED-INPUTS-V2-20260909` |
| A-MAP | `deliverables/yunfan-delivery-receipt.md` | `c8afa5315ea6594ac2c9ec9ec4766008eaa5b8ca82f7e0f6997327df7ed8ea10`；冻结的复核前 `6/4` 映射 |
| E-MAP | `deliverables/yunfan-evidence-map-v3-verification.json` | `69a659909671a6d02ef7feba6cc8c13e995b496e6bed676c1c2800d42c9543c0`；`YF-MAP-CRITERIA-MATRIX-V3-20260909` |
| A-REVIEW | `deliverables/yf-independent-review.md` | `223f8d1727a2abc95c7210993e26b0b9ec82bf95627c33081ea29cfde33f4a79` |
| E-REVIEW | `.work-receipts/reviewer-report-verification-v2.json` | `bea143eac208fc95cb49d01cd8d5410e3e03607eb8a56a5813d31f8704265d4d`；`YF-REVIEW-INDEPENDENT-V3-20260909` |
| P-OBSERVE | `review-inputs/usage-process-observation.json` | `29fa1f377f792204c13811480e515808563091f2b1f0691020d445f41e74093d` |
| A-FINAL | `deliverables/yf-delivery.md` | 由 `deliverables/yf-delivery-verification.json` 和最终 AWR 证据记录绑定 |

AWR 中较早的 `locally_verified` 证据与旧判断继续作为历史记录存在；当前语义结论采用上表所列的 V2/V3 纠正版及独立复核证据。AWR 的 `currency=current` 表示证据仍绑定同一 Git 来源 SHA，不表示较早文本自动取代后来的事实纠正。

## 十条验收要求的最终对应

| 编号 | 验收原文 | 当前证据与执行方式 | 来源版本 | 判断 |
| --- | --- | --- | --- | --- |
| G-1 | 区分文件存在检查、实际使用验证和独立复核。 | A-GAPS 分层记录；E-GAPS 运行确定性结构检查并由 AWR 绑定哈希。 | Git 基线；A-GAPS、E-GAPS 见上表 | 满足 |
| G-2 | 保留实际产物与来源引用，无法确认的内容显式说明。 | A-GAPS 引用草案、输入、使用产物和缺失事实；E-GAPS 验证对应章节。 | Git 基线；S-DRAFT、S-INPUT、A-GAPS、E-GAPS | 满足 |
| U-1 | 在实际客户端中使用交付文档完成输入案例，保留过程及返工。 | 当前 Agent 处理三条输入并保留三条说明修订链；六个问题标记为 Agent 自拟，用户后续追问为零；E-USE 验证结构与哈希。 | Git 基线；S-DRAFT、S-INPUT、A-USE、E-USE | 满足文档协助使用；不证明用户多轮互动或真实系统成功 |
| U-2 | 保留实际产物与来源引用，无法确认的内容显式说明。 | A-USE 每案分列草案事实、Agent 建议、未知项、实际修订和未执行操作；E-USE 绑定产物。 | Git 基线；A-USE、E-USE | 满足 |
| M-1 | 每条验收有实际证据引用，缺证据时不得宣告完成。 | A-MAP 冻结十行 `6/4`；E-MAP 校验映射完整性；本表仅在新证据实际形成后更新 R/D 状态。 | Git 基线；A-MAP、E-MAP；本文件 | 满足 |
| M-2 | 保留实际产物与来源引用，无法确认的内容显式说明。 | A-MAP 逐行记录执行方式、版本和未知项；E-MAP 绑定冻结版本；本文件保留后续证据链。 | Git 基线；A-MAP、E-MAP；本文件 | 满足 |
| R-1 | 核对证据对应关系与实际使用记录，指出不准确结论。 | 不同参与者读取五阶段发布资料和过程观察，检查 94 个发布文件、75 个声明哈希、三份说明和十行映射；A-REVIEW 记录结论，E-REVIEW 机器验证并由 AWR 完成 YF-REVIEW。 | Git 基线；A-REVIEW、E-REVIEW | 满足 |
| R-2 | 保留实际产物与来源引用，无法确认的内容显式说明。 | A-REVIEW 保留输入、产物、哈希、历史错误、失败回执及真实系统/客户来源边界；E-REVIEW 绑定复核产物。 | Git 基线；A-REVIEW、E-REVIEW | 满足 |
| D-1 | 处理复核意见并生成可追溯完成回执。 | 执行者接受“无需返工、保持边界”的复核结论，形成本文件；最终机器报告和 AWR `work.completed` 回执绑定本文件与两条验收。 | Git 基线；本文件；`YF-DELIVER-FINAL-MATERIAL-PACKAGE-20260909`；`.work-receipts/yf-deliver-completion-event.json` | 满足；仅指材料交付 |
| D-2 | 保留实际产物与来源引用，无法确认的内容显式说明。 | 本文件汇总当前说明、版本哈希、复核结果、历史纠正、未确认事实、责任人和系统操作边界；最终机器报告执行结构、哈希和边界断言。 | Git 基线；本文件；`deliverables/yf-delivery-verification.json` | 满足 |

复核前 A-MAP 的 `6/4`、复核完成后的 `8/2` 和本次完成后的 `10/10` 分属不同时间点，均保留且不互相覆盖。`10/10` 只表示五个工作项的十条材料验收要求均有证据，不授予客户 UAT、真实系统验收或整场 E4。

## 独立复核结果及保留边界

独立复核的正式结论是：当前 `process-correction` 版本通过，生产者无需重做三份主产物。复核确认三份主产物与发布快照逐字节一致，并确认三份说明足以用于“文档协助使用”，同时继续保留下列限制：

- 三条输入是项目提供的验收输入，客户现场来源未知。
- 使用者没有针对三条输入参与后续问答；六个检查问题均由 Agent 构造。
- 帮助草案和输入表没有修改；发生的是说明、报告、证据映射和工作记录修订。
- 没有登录另一个帮助中心网站，也没有进行真实账号、导入或反馈操作。
- 独立复核只支持 YF-REVIEW；最终材料交付由本次 YF-DELIVER 单独完成。

## 原始版本、纠正问题与失败记录

`review-inputs/` 中继续保留 `initial`、`usage-scope-clarification`、`round-1`、`round-2`、`process-correction` 五个阶段及说明文件；当前三份主产物保留纠正版，旧版本由阶段快照和哈希链固定，没有覆盖或删除。

1. 初始阶段错误地把外部帮助中心入口、环境和账号当作 YF-USE 前置条件，并将工作阻塞；后续范围澄清撤回该判断。原 `work.blocked` 回执和 incomplete 会话保留。
2. round-1 与 round-2 曾把输入表描述为稍后新增，混淆“材料到达”和“执行者发现”；process-correction 已更正为输入表在该轮开始前已提供。
3. round-1 与 round-2 曾把六个 Agent 自拟问题写成“追问”或“两轮返工”，不足以区分用户参与；process-correction 已更正为 `3` 条请求、`6` 个 Agent 检查问题、`0` 次用户后续追问。
4. 旧版 AWR 证据和冻结的 A-MAP 继续保留为当时结论，不反向改写；当前结论采用 V2/V3 证据链。
5. 三个生产者历史会话仍为 `incomplete`：`01M21TEK3RJ1QV2YFW7K1QZWKQ`、`01M21V96JS4PYZS8K2JWYYCWXQ`、`01M21XAD13ZHWFB5A2NEDC080P`，不伪造为正常结束。
6. 复核者首次机器检查的相对路径错误及三次 session 参数错误保存在 `.work-receipts/reviewer-verification-failure.json`、`.work-receipts/reviewer-session-inspection-failures.json`；后续已用绝对路径和正确位置参数重跑通过。
7. 复核者两次完成门失败保存在 `.work-receipts/reviewer-complete.json` 和 `.work-receipts/reviewer-complete-success.json`；原因分别是 evidence locator 指向 Markdown、机器报告缺少顶层 `command`，后续 V3 证据和完成回执成功。

## 未确认事实与后续责任

| 责任方 | 仍需补齐的事实或操作 | 交接产物 / 完成条件 |
| --- | --- | --- |
| 系统负责人或管理员 | 账号申请入口、正式字段、审核者映射、审核规则、时限、紧急规则 | 发布受控的账号申请说明；如需验收，实际提交并保留申请/审批/开通回执 |
| 导入功能负责人和数据所有者 | CSV 模板/schema、编码、字段、限制、去重、错误处理、回滚、目标环境、数据授权和成功标准 | 发布受控导入规范；在获批安全范围执行最小样本并保留文件哈希、环境、时间、结果和回滚证据 |
| 支持或工单负责人 | 正式反馈渠道、联系人、SLA、附件与日志共享政策、关闭标准 | 发布受控反馈规范；实际提交后保留工单、响应、处理与关闭回执 |
| 验收组织者 / 客户责任方 | 三条输入是否来自客户现场；是否需要客户 UAT 或完整 E4 | 提供可核验来源；若需要，另立真实验收任务，使用自然业务语言并由实际用户客户端执行 |
| 后续真实系统执行者 | 明确目标系统、环境、账号、权限、数据和写入授权 | 只在边界全部确认后执行真实操作；本次材料回执不得替代系统回执 |
| 材料维护者 | 将责任方确认的系统事实纳入受控帮助材料并维护版本 | 修改源材料后重新执行文档协助使用、逐条映射和不同参与者复核；旧证据不自动沿用 |

这些事实没有因为本次材料交付而自动解决；它们是后续真实系统操作或更高等级验收的前置条件，不是本次文档交付的阻塞项。

## 可追溯回执索引

- 独立复核产物：`deliverables/yf-independent-review.md`
- 独立复核机器证据：`.work-receipts/reviewer-report-verification-v2.json`
- 最终交付产物：`deliverables/yf-delivery.md`
- 最终交付机器证据：`deliverables/yf-delivery-verification.json`
- 本次会话开始：`.work-receipts/yf-deliver-session-start.json`
- 本次完整上下文：`.work-receipts/yf-deliver-context-l1.json`
- 本次工作推进：`.work-receipts/yf-deliver-progress.json`
- 最终证据登记：`.work-receipts/yf-deliver-final-evidence-event.json`
- 两条验收完成输入：`.work-receipts/yf-deliver-completion-input.json`
- AWR 完成事件：`.work-receipts/yf-deliver-completion-event.json`
- 最终项目状态：`.work-receipts/yf-deliver-final-status.json`
- AWR 一致性检查：`.work-receipts/yf-deliver-doctor.json`
- 会话 checkpoint 与结束：`.work-receipts/yf-deliver-checkpoint.json`、`.work-receipts/yf-deliver-session-end.json`

最终状态以这些不可变回执中的实际事件 ID、项目修订和内容哈希为准。本轮未进行 Git 提交或推送；交付以当前工作区产物和 AWR 回执为边界。
