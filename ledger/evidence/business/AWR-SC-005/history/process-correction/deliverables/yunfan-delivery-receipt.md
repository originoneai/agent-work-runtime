# 云帆帮助中心逐条验收证据映射与当前状态回执

## 结论

- 记录日期：2026-09-09
- Git 源码基线：`ed1368bf06806ff3227af899129d4914d5fe81e0`
- 映射取证快照：AWR 项目修订 `420`；权威工作台账来源 `r47`。该编号是本次映射修订前的冻结取证版本，不冒充证据登记与完成动作之后的可变“最新状态”。
- 映射完整性：`10/10` 条工作项验收要求均有独立行；其中 `6` 条有执行者侧证据并判断满足，`4` 条因独立复核或最终交付尚未发生而明确未满足。
- 当前交付判断：**尚不具备最终交付条件。**
- 实际使用边界：当前 Agent 客户端已处理 `3` 条真实收到的项目请求；后续 `6` 个检查问题均由 Agent 为检查说明而构造，使用者参与的后续追问为 `0`；实际返工是报告说明文字的修订，源材料修改为 `0`；真实系统操作 `0/3`；不同参与者独立复核 `0`；客户 UAT 来源无法确认。

本回执是执行者侧的证据映射和状态判断，不是 `YF-REVIEW` 的独立复核记录，也不是 `YF-DELIVER` 的最终交付产物。AWR 在本文件冻结后发生的证据登记和工作状态变化，以对应的不可变事件回执及最终状态查询为准。

## 本轮修订的不准确结论

1. 原回执把 AWR `r183` 和“`YF-MAP` 正在形成”写成当前状态；现改为明确标注的冻结取证快照，不再用历史修订号代表后续状态。
2. 原 M-1、M-2 写为“待本项登记”；映射内容现已按 `RULES.md` 来源 `r2` 重新验证，登记结果由配套 AWR 证据与完成事件承载。
3. 根据用户本轮澄清，`materials/usage-inputs.csv` 在上一轮开始前已经提供。此前“请求尚未提供”和“随后新增”的说法源于执行者漏检，混淆了材料到达时间与发现时间，现已撤回。
4. 原使用过程把每案两个 Agent 自拟检查问题写成仿佛由使用者发起的两轮追问。现改为 `3` 条真实收到的请求、`6` 个 Agent 构造的检查问题、`0` 轮使用者后续追问；模拟对话不再作为用户互动证据。
5. 实际材料修订只发生在执行者编写的说明报告、缺口清单、证据映射、配套验证与 AWR 工作记录；`materials/help-draft.md` 和 `materials/usage-inputs.csv` 均未修改。
6. `materials/local-check.md` 只代表文件存在检查，不能证明输入表何时到达，也不能抵消后续 Agent 使用证据或抵充独立复核。
7. “`3/3` 条输入处理完成”只说明 Agent 使用过程及说明修订记录完整，不代表用户多轮互动、客户 UAT、真实系统成功或最终交付通过。

## 来源版本索引

| 版本标识 | 来源或产物 | 冻结版本 |
| --- | --- | --- |
| S-GOAL | `GOALS.md:1-3` | AWR source `r1`；SHA-256 `6422d45f96484905337108d38bd7077de94f0c8cf94f02a092eda6e04bb91d3f` |
| S-PLAN | `PLAN.md:1-3` | AWR source `r1`；SHA-256 `0a7f619a201e82337b62680d8f7f9a869180e906f3c766081bc6a42e0cfdf8e1` |
| S-RULES | `RULES.md:1-11` | AWR source `r2`；SHA-256 `0d685c2f575e04fba4892a56ca4f7ff8caf9abeff2090c391f0490b9e28cec36` |
| S-LEDGER | `work-ledger.yaml` | AWR source `r47`；SHA-256 `312fdc6154d9fcf8630817463d057c980caf88cad41529ed1de227842988a3da`；本次映射修订前冻结基线 |
| D-CONTRACT | `DELIVERABLES.md` | SHA-256 `990bb1e4842926fc666cfc0cd94598609d3280c6ca7924f73c5ac7f98be2e0a8` |
| A-DRAFT | `materials/help-draft.md:1-3` | SHA-256 `bbb1980471ead2a84b1c641c0536ff81d547ec0beac7366db1439ad091520941` |
| A-LOCAL | `materials/local-check.md:1-2` | SHA-256 `f31020534185182cda3cb408fed52fc26708f532e12b64763614b7d1d8f227a4` |
| A-INPUTS | `materials/usage-inputs.csv:1-4` | SHA-256 `b3fd3f7e16405a84fea59bbefb70d5ada50d7659a8cb7274d6f65be51897798f` |
| A-GAPS | `deliverables/yunfan-evidence-gaps.md` | SHA-256 `60c9cdc0ad0ef3437c6666c94dbc9a5161c3bb0e370ef4b8a35d1a39899e6d14` |
| E-GAPS | `deliverables/yunfan-evidence-gaps-current-v3-verification.json` | SHA-256 `f8e25f0b5e8e7caec1a0af9fdb84f3f77e1fd51db8efd173dec9d0212fc34477`；AWR 键 `YF-GAPS-CURRENT-STATE-CORRECTION-V3-20260909` |
| A-USE | `deliverables/yunfan-usage-report.md` | SHA-256 `a8230b62891dbb0e2dc5eeafa302978692472ebb8fc7c04d2900de6d45420400` |
| E-USE | `deliverables/yunfan-provided-inputs-v2-verification.json` | SHA-256 `e259316cf0665826f10c8170995752495f567dfe4e0f25a5d4c6b97da5d65bd9`；AWR 键 `YF-USE-PROVIDED-INPUTS-V2-20260909` |

执行方式口径：

- “当前 Agent 读取/比对”指直接读取本项目文件并核对文本、行号、哈希和 AWR 来源版本；没有登录其他网站。
- “当前 Agent 实际使用”指当前 Agent 客户端逐条处理 A-INPUTS 的三条真实请求，以 A-DRAFT 为资料形成初版说明，再由同一 Agent 构造每案两个检查问题并修订说明；检查问题不是使用者消息，使用者参与的后续追问为 `0`。
- “实际修订”指 A-USE 中说明文字的编写和修订，以及本次受影响的执行者侧证据与工作记录更新；不包括 A-DRAFT、A-INPUTS 或真实业务系统修改。
- “确定性本地校验”指使用报告内登记的 `rtk proxy awk` 结构断言，以及 `jq` 解析和 `shasum -a 256` 内容绑定；它不等于独立复核。
- “未执行核验”指通过文件清单和 AWR 工作状态确认所需产物/事件不存在；不存在本身只支持“未满足”，不能支持通过。

## 十条验收要求逐项映射

| 编号 | 工作项与验收原文 | 实际证据 | 执行方式 | 来源版本 | 证据层级 | 判断 |
| --- | --- | --- | --- | --- | --- | --- |
| G-1 | `YF-GAPS`：区分文件存在检查、实际使用验证和独立复核。 | A-GAPS“当前证据层级核对”；E-GAPS 第一项检查；A-LOCAL 作为文件检查历史基线。 | 当前 Agent 读取并逐层比对，另行区分真实请求、Agent 自拟检查问题和实际报告修订；执行 E-GAPS 的确定性断言；AWR 按完整源码 SHA 核验报告哈希。 | S-LEDGER@r47；S-RULES@r2；A-GAPS、E-GAPS、A-LOCAL 见版本索引。 | locally_verified | **满足** |
| G-2 | `YF-GAPS`：保留实际产物与来源引用，无法确认的内容显式说明。 | A-GAPS“实际产物、来源与未知边界”；E-GAPS 第二项检查。 | 当前 Agent 对 A-DRAFT、A-INPUTS、A-USE、A-LOCAL 和缺失的复核/最终产物逐项核对并哈希绑定；输入时间仅按用户澄清记录，不伪造文件时间证据。 | S-LEDGER@r47；A-GAPS、E-GAPS、A-DRAFT、A-INPUTS、A-USE、A-LOCAL 见版本索引。 | locally_verified | **满足** |
| U-1 | `YF-USE`：在实际客户端中使用交付文档完成输入案例，保留过程及返工。 | A-INPUTS 第 2-4 行；A-USE 的 U-01 至 U-03 与“过程角色说明”；E-USE 第一项检查。 | 当前 Agent 客户端逐条处理三条真实请求；同一 Agent 构造每案两个检查问题并实际修订说明；使用者后续追问为 `0`，不把模拟问题当作用户互动；执行 E-USE 结构断言。 | S-LEDGER@r47；A-DRAFT、A-INPUTS、A-USE、E-USE 见版本索引。 | locally_verified | **满足当前 Agent 使用及说明返工门槛；不证明用户多轮互动** |
| U-2 | `YF-USE`：保留实际产物与来源引用，无法确认的内容显式说明。 | A-USE 每个案例的“来源、修订与边界”、跨案例矩阵和 `YF-USE` 映射；E-USE 第二项检查。 | 当前 Agent 将草案事实、风险控制建议、实际报告文字修订、未知系统事实和未执行操作分栏记录；输入、输出和验证报告均以 SHA-256 绑定，源材料修改为 `0`。 | S-LEDGER@r47；S-RULES@r2；A-DRAFT、A-INPUTS、A-USE、E-USE 见版本索引。 | locally_verified | **满足** |
| M-1 | `YF-MAP`：每条验收有实际证据引用，缺证据时不得宣告完成。 | 本表 G-1 至 D-2 共十行；R-1、R-2、D-1、D-2 明确记录无通过证据。 | 当前 Agent 将 S-LEDGER 的十条原文逐项交叉映射；确定性校验统计十行、六条满足和四条未满足，并校验三类过程事实，不把缺证据行写成通过。 | S-LEDGER@r47；S-RULES@r2；本回执由 V3 配套验证按冻结后 SHA-256 绑定。 | locally_verified | **满足映射内容要求** |
| M-2 | `YF-MAP`：保留实际产物与来源引用，无法确认的内容显式说明。 | “来源版本索引”“执行方式口径”“本轮修订的不准确结论”“未决事实与边界”及本表。 | 当前 Agent 记录每行的实际证据、执行方式、来源版本、证据层级和判断；使用 `shasum`、`jq` 与确定性表格断言复核，并保留输入时间依据和未确认事实。 | S-GOAL@r1；S-PLAN@r1；S-RULES@r2；S-LEDGER@r47；各产物版本见索引。 | locally_verified | **满足** |
| R-1 | `YF-REVIEW`：核对证据对应关系与实际使用记录，指出不准确结论。 | 无独立复核证据；`deliverables/yf-independent-review.md` 不存在；AWR `YF-REVIEW` evidence 为空。 | **未执行**；仅由当前执行者检查文件缺失和 AWR 状态并修订自己的结论，不能自签为“不同参与者”。 | S-PLAN@r1；S-RULES@r2；S-LEDGER@r47；D-CONTRACT。 | none | **未满足，不宣告完成** |
| R-2 | `YF-REVIEW`：保留实际产物与来源引用，无法确认的内容显式说明。 | 无独立复核产物，因而无可用于通过本条的复核者来源引用。 | **未执行**；文件缺失检查只能证明当前无产物。 | S-LEDGER@r47；D-CONTRACT。 | none | **未满足，不宣告完成** |
| D-1 | `YF-DELIVER`：处理复核意见并生成可追溯完成回执。 | 无复核意见可处理；`deliverables/yf-delivery.md` 不存在；AWR 显示依赖 `YF-REVIEW` 未完成。 | **未执行**；通过 AWR 依赖查询和文件缺失检查确认阻塞，不生成虚假完成回执。 | S-LEDGER@r47；S-PLAN@r1；D-CONTRACT。 | none | **未满足，不宣告完成** |
| D-2 | `YF-DELIVER`：保留实际产物与来源引用，无法确认的内容显式说明。 | 当前只有预交付材料，没有最终交付产物。 | **未执行**；当前材料只用于记录缺口，不能抵充最终产物。 | S-LEDGER@r47；S-RULES@r2；D-CONTRACT。 | none | **未满足，不宣告完成** |

## 项目硬规则对应

| 硬规则 | 对应落实 |
| --- | --- |
| `RULES.md:3`：未知或缺证据的内容显式保留，不把计划、样例或文件存在当通过。 | R-1 至 D-2 保持未满足；“未决事实与边界”保留客户来源、系统事实和真实终态未知。 |
| `RULES.md:7`：本地检查不能替代实际使用；完成前需要逐条映射和不同执行者复核。 | U-1 使用 A-INPUTS、当前 Agent 的实际处理及说明修订记录，不使用 A-LOCAL 抵充；模拟问题明确不算用户互动；十条均已映射；独立复核仍未完成。 |
| `RULES.md:11`：映射注明执行方式和来源版本；无法证明实际使用的条目保持未完成。 | 本表为每行设置“执行方式”“来源版本”；U-1 引用实际 Agent 过程，R/D 缺证据行保持未满足。 |

## 未决事实与边界

1. A-INPUTS 在上一轮开始前已经提供，这一先后关系来自用户本轮明确说明；工作区材料不能独立还原精确创建时间。现有来源也不能证明其来自客户现场，因此不得称为客户 UAT。
2. A-USE 中六个检查问题均由 Agent 构造，不是使用者消息；使用者后续参与为 `0`，不得据此声称用户多轮互动。
3. A-DRAFT 与 A-INPUTS 没有被使用过程修改；实际修改的是执行者编写的报告与证据/工作记录。
4. A-DRAFT 没有账号申请入口、正式字段、审核者映射、审核标准或时限。
5. A-DRAFT 没有 CSV 编码、分隔符、表头/schema、大小限制、去重、回滚或成功判定规则。
6. A-DRAFT 没有问题反馈入口、具体联系人、SLA、数据共享政策或正式关闭标准。
7. 当前没有真实账号申请、导入或反馈回执；三条说明产物不能替代真实系统成功证据。
8. 独立复核必须由不同参与者执行；当前执行者只修订自己的材料，不签署 `yf-independent-review.md`。
9. 本轮新增映射验证只能支持 `YF-MAP`；不能据此完成 `YF-REVIEW` 或 `YF-DELIVER`。

## 完成状态判定口径

- `YF-GAPS`、`YF-USE`：已有纠正过程归因和输入时间的版本化、哈希绑定 `locally_verified` 证据；旧证据保留为历史，不再作为最新结论。
- `YF-MAP`：本回执已按最新规则补齐执行方式、来源版本和十条判断；其 AWR 完成状态必须以本文件冻结后的新版证据登记及 `work.completed` 回执为准。
- `YF-REVIEW`：必须由不同参与者执行，当前证据为零。
- `YF-DELIVER`：依赖 `YF-REVIEW`，当前仍阻塞。
- 项目里程碑：在独立复核和最终交付完成前保持 `in_progress`，最终交付结论保持 No-Go。
