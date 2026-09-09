# 云帆帮助中心逐条验收证据映射与当前状态回执

## 当前判断

- 记录日期：2026-09-09
- Git 源码基线：`ed1368bf06806ff3227af899129d4914d5fe81e0`
- AWR 状态快照：项目修订 `183`；`YF-GAPS`、`YF-USE` 已完成，`YF-MAP` 正在形成本映射。
- 当前交付判断：**尚不具备最终交付条件**。
- 已补齐：证据缺口核对；当前 Agent 客户端对项目提供的三条输入进行实际处理；每条输入的两轮返工、最终说明、来源和未知边界。
- 尚未补齐：不同参与者独立复核；复核意见处理；最终可追溯交付。
- 真实系统操作：`0`；没有创建账号、导入 CSV/JSON 或提交问题反馈。

本回执是证据映射和当前状态记录，不是独立复核记录，也不是最终交付回执。没有证据的条件明确标为“未满足”，不会从依赖关系、计划或文件存在推定通过。

## 权威来源与实际产物

| 类型 | 路径或标识 | 绑定信息 | 用途 |
| --- | --- | --- | --- |
| 目标 | `GOALS.md:1-3` | AWR source r1，fingerprint `6422d45f…91d3f` | 定义补齐实际使用、逐条证据和完成交付 |
| 计划 | `PLAN.md:1-3` | AWR source r1，fingerprint `0a7f619a…df8e1` | 要求不同执行者独立复核 |
| 规则 | `RULES.md:1-7` | AWR source r1，fingerprint `4205cb30…b2d28` | 禁止把文件检查当实际使用或最终完成 |
| 工作台账 | `work-ledger.yaml` | AWR source r20，fingerprint `edb0f94c…7a10c` | 定义五项工作、依赖、验收和状态 |
| 交付草案 | `materials/help-draft.md:1-3` | SHA-256 `bbb1980471ead2a84b1c641c0536ff81d547ec0beac7366db1439ad091520941` | 当前 Agent 回答的业务材料 |
| 使用输入 | `materials/usage-inputs.csv:1-4` | SHA-256 `b3fd3f7e16405a84fea59bbefb70d5ada50d7659a8cb7274d6f65be51897798f` | 三条项目提供的实际使用请求 |
| 使用输出 | `deliverables/yunfan-usage-report.md` | SHA-256 `bfa30a29d86606a2723737f560c3c2751292130aab945324083e1b47d87af9c9` | 三条完整问答、返工、最终说明和边界 |
| 使用验证 | `deliverables/yunfan-provided-inputs-verification.json` | SHA-256 `72e42d7eb94d562ffbe55040add5a827edfd4bb1a3cf4b4f7c02ca2e084720f0` | `YF-USE-PROVIDED-INPUTS-20260909`，`locally_verified` |
| 缺口报告 | `deliverables/yunfan-evidence-gaps.md` | SHA-256 `64cdf2bb1267c9f64bc2f6e16c5294557429181cfe7f4a7d3f4090f9a1a5e568` | 区分证据层级和保留未知项 |
| 范围验证 | `deliverables/yunfan-evidence-gaps-scope-correction-verification.json` | SHA-256 `5a901d81478c4e2f691521c343a39bf1a18f0ef7daf6cc9ff5cee6371165c2fd` | `YF-GAPS-SCOPE-CORRECTION-20260909`，`locally_verified` |

以上短 fingerprint 仅用于人工识别；完整来源指纹保留在 AWR。可变的工作台账不作为机器验证报告，机器证据使用 `deliverables/` 下的不可变内容哈希。

## 逐条验收映射

| 编号 | 工作项与验收原文 | 实际证据 | 证据层级 | 判断 |
| --- | --- | --- | --- | --- |
| G-1 | `YF-GAPS`：区分文件存在检查、实际使用验证和独立复核。 | `yunfan-evidence-gaps.md`；`yunfan-evidence-gaps-scope-correction-verification.json`；AWR 键 `YF-GAPS-SCOPE-CORRECTION-20260909` | locally_verified | 满足 |
| G-2 | `YF-GAPS`：保留实际产物与来源引用，无法确认的内容显式说明。 | 同上；缺口报告列出业务来源、可确认事实、未知项和真实操作边界 | locally_verified | 满足 |
| U-1 | `YF-USE`：在实际客户端中使用交付文档完成输入案例，保留过程及返工。 | `usage-inputs.csv:2-4` 与使用报告 U-01 至 U-03；每项含首次回答、两轮追问/返工和最终回答；AWR 键 `YF-USE-PROVIDED-INPUTS-20260909` | locally_verified | 满足当前 Agent 使用过程门槛 |
| U-2 | `YF-USE`：保留实际产物与来源引用，无法确认的内容显式说明。 | 使用报告三份最终说明；每项的“来源与边界”；输入、草案、输出和验证报告哈希 | locally_verified | 满足 |
| M-1 | `YF-MAP`：每条验收有实际证据引用，缺证据时不得宣告完成。 | 本表 G-1 至 D-2；R-1、R-2、D-1、D-2 均明确无完成证据并判为未满足 | locally_verified（待本项登记） | 满足映射要求 |
| M-2 | `YF-MAP`：保留实际产物与来源引用，无法确认的内容显式说明。 | 本回执“权威来源与实际产物”“未决事实与边界”及配套机器验证 | locally_verified（待本项登记） | 满足 |
| R-1 | `YF-REVIEW`：核对证据对应关系与实际使用记录，指出不准确结论。 | 无；`deliverables/yf-independent-review.md` 尚不存在，且当前执行者不得自签独立复核 | none | **未满足，不宣告完成** |
| R-2 | `YF-REVIEW`：保留实际产物与来源引用，无法确认的内容显式说明。 | 无独立复核产物 | none | **未满足，不宣告完成** |
| D-1 | `YF-DELIVER`：处理复核意见并生成可追溯完成回执。 | 无；复核尚未发生，无法处理复核意见，`deliverables/yf-delivery.md` 尚不存在 | none | **未满足，不宣告完成** |
| D-2 | `YF-DELIVER`：保留实际产物与来源引用，无法确认的内容显式说明。 | 当前仅有预交付材料，不能替代最终交付产物 | none | **未满足，不宣告完成** |

## 实际使用证据摘要

- 原始输入一：`说明账号申请的分工`。最终产物给出申请者、审核者、提交前确认和回执保存清单；入口、字段、审核规则仍未知。
- 原始输入二：`准备一个 CSV 导入说明`。最终产物确认 CSV 受支持，并给出先确认模板/schema、去敏最小样本、获授权后试导入和保存回执的有条件步骤；编码、字段和系统限制仍未知。
- 原始输入三：`整理问题反馈所需材料`。最终产物给出复现步骤、证据、联系角色和建议关闭检查；渠道、具体联系人、SLA 和正式关闭标准仍未知。
- 三项均在当前 Agent 客户端形成完整过程；三项真实系统终态均未执行。

## 未决事实与边界

1. `materials/usage-inputs.csv` 是项目提供的验收输入，但现有来源不能证明其来自客户现场，因此不得称为客户 UAT。
2. 草案没有账号申请入口、正式字段、审核者映射、审核标准或时限。
3. 草案没有 CSV 编码、分隔符、表头/schema、大小限制、去重、回滚或成功判定规则。
4. 草案没有问题反馈入口、具体联系人、SLA、数据共享政策或正式关闭标准。
5. 当前没有真实账号申请、导入或反馈回执；说明产物不能替代真实系统成功证据。
6. 独立复核必须由不同参与者执行；当前执行者只准备材料，不签署 `yf-independent-review.md`。

## 后续动作

1. 将本映射及其机器验证登记到 `YF-MAP`。
2. 由不同参与者执行 `YF-REVIEW`，逐项核对输入、回答、返工、来源和结论，并记录任何不准确内容。
3. 当前执行者只在收到独立复核结果后处理意见并推进 `YF-DELIVER`；在此之前维持“尚不具备最终交付条件”。
