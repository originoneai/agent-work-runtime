# 云帆帮助中心逐条验收证据映射与当前状态回执

## 结论

- 记录日期：2026-09-09
- Git 源码基线：`ed1368bf06806ff3227af899129d4914d5fe81e0`
- 映射取证快照：AWR 项目修订 `240`；权威工作台账来源 `r26`。该编号是本次映射的冻结取证版本，不冒充后续可变的“最新状态”。
- 映射完整性：`10/10` 条工作项验收要求均有独立行；其中 `6` 条有执行者侧证据并判断满足，`4` 条因独立复核或最终交付尚未发生而明确未满足。
- 当前交付判断：**尚不具备最终交付条件。**
- 实际使用边界：当前 Agent 客户端已处理项目提供的三条请求并保留两轮返工；真实系统操作 `0/3`；不同参与者独立复核 `0`；客户 UAT 来源无法确认。

本回执是执行者侧的证据映射和状态判断，不是 `YF-REVIEW` 的独立复核记录，也不是 `YF-DELIVER` 的最终交付产物。AWR 在本文件冻结后发生的证据登记和工作状态变化，以对应的不可变事件回执及最终状态查询为准。

## 本轮修订的不准确结论

1. 原回执把 AWR `r183` 和“`YF-MAP` 正在形成”写成当前状态；现改为明确标注的冻结取证快照，不再用历史修订号代表后续状态。
2. 原 M-1、M-2 写为“待本项登记”；映射内容现已按 `RULES.md` 来源 `r2` 重新验证，登记结果由配套 AWR 证据与完成事件承载。
3. 原缺口材料中的“具体使用请求尚未提供”已过时；项目现已提供三条输入，并已有当前 Agent 的完整处理记录。
4. `materials/local-check.md` 只代表早期文件存在检查，不能抵消后续实际使用证据，也不能抵充独立复核。
5. “`3/3` 条输入处理完成”只说明 Agent 使用过程记录完整，不代表客户 UAT、真实系统成功或最终交付通过。

## 来源版本索引

| 版本标识 | 来源或产物 | 冻结版本 |
| --- | --- | --- |
| S-GOAL | `GOALS.md:1-3` | AWR source `r1`；SHA-256 `6422d45f96484905337108d38bd7077de94f0c8cf94f02a092eda6e04bb91d3f` |
| S-PLAN | `PLAN.md:1-3` | AWR source `r1`；SHA-256 `0a7f619a201e82337b62680d8f7f9a869180e906f3c766081bc6a42e0cfdf8e1` |
| S-RULES | `RULES.md:1-11` | AWR source `r2`；SHA-256 `0d685c2f575e04fba4892a56ca4f7ff8caf9abeff2090c391f0490b9e28cec36` |
| S-LEDGER | `work-ledger.yaml` | AWR source `r26`；SHA-256 `7f20a24d8c34894f9b9a9cb6e598b5928c5340b5c58106843042b87f71c4ce59` |
| D-CONTRACT | `DELIVERABLES.md` | SHA-256 `990bb1e4842926fc666cfc0cd94598609d3280c6ca7924f73c5ac7f98be2e0a8` |
| A-DRAFT | `materials/help-draft.md:1-3` | SHA-256 `bbb1980471ead2a84b1c641c0536ff81d547ec0beac7366db1439ad091520941` |
| A-LOCAL | `materials/local-check.md:1-2` | SHA-256 `f31020534185182cda3cb408fed52fc26708f532e12b64763614b7d1d8f227a4` |
| A-INPUTS | `materials/usage-inputs.csv:1-4` | SHA-256 `b3fd3f7e16405a84fea59bbefb70d5ada50d7659a8cb7274d6f65be51897798f` |
| A-GAPS | `deliverables/yunfan-evidence-gaps.md` | SHA-256 `08b47a96b29995466b106232b3289df8552a4e53d89a88e82437980d98ddd4da` |
| E-GAPS | `deliverables/yunfan-evidence-gaps-current-v2-verification.json` | SHA-256 `4a5f9383572157b0ff916e3f706cd9638db5372284f38d065a917abf250eca5e`；AWR 键 `YF-GAPS-CURRENT-STATE-CORRECTION-V2-20260909` |
| A-USE | `deliverables/yunfan-usage-report.md` | SHA-256 `bfa30a29d86606a2723737f560c3c2751292130aab945324083e1b47d87af9c9` |
| E-USE | `deliverables/yunfan-provided-inputs-verification.json` | SHA-256 `72e42d7eb94d562ffbe55040add5a827edfd4bb1a3cf4b4f7c02ca2e084720f0`；AWR 键 `YF-USE-PROVIDED-INPUTS-20260909` |

执行方式口径：

- “当前 Agent 读取/比对”指直接读取本项目文件并核对文本、行号、哈希和 AWR 来源版本；没有登录其他网站。
- “当前 Agent 实际使用”指在本轮 Agent 客户端逐条处理 A-INPUTS，以 A-DRAFT 为资料，保留首次回答、两轮追问或返工及最终说明。
- “确定性本地校验”指使用报告内登记的 `rtk proxy awk` 结构断言，以及 `jq` 解析和 `shasum -a 256` 内容绑定；它不等于独立复核。
- “未执行核验”指通过文件清单和 AWR 工作状态确认所需产物/事件不存在；不存在本身只支持“未满足”，不能支持通过。

## 十条验收要求逐项映射

| 编号 | 工作项与验收原文 | 实际证据 | 执行方式 | 来源版本 | 证据层级 | 判断 |
| --- | --- | --- | --- | --- | --- | --- |
| G-1 | `YF-GAPS`：区分文件存在检查、实际使用验证和独立复核。 | A-GAPS“当前证据层级核对”；E-GAPS 两项检查之一；A-LOCAL 作为历史文件检查基线。 | 当前 Agent 读取并逐层比对；执行 E-GAPS 中的确定性结构断言；AWR 按完整源码 SHA 核验报告内容哈希为 current。 | S-LEDGER@r26；S-RULES@r2；A-GAPS、E-GAPS、A-LOCAL 见版本索引。 | locally_verified | **满足** |
| G-2 | `YF-GAPS`：保留实际产物与来源引用，无法确认的内容显式说明。 | A-GAPS“实际产物、来源与未知边界”；E-GAPS 第二项检查。 | 当前 Agent 对 A-DRAFT、A-INPUTS、A-USE、A-LOCAL 和缺失的独立复核/最终产物逐项核对并哈希绑定。 | S-LEDGER@r26；A-GAPS、E-GAPS、A-DRAFT、A-INPUTS、A-USE、A-LOCAL 见版本索引。 | locally_verified | **满足** |
| U-1 | `YF-USE`：在实际客户端中使用交付文档完成输入案例，保留过程及返工。 | A-INPUTS 第 2-4 行；A-USE 的 U-01 至 U-03；E-USE 第一项检查。 | 当前 Agent 客户端逐条处理三条原始输入；每条保留首次回答、两轮有业务意义的追问/修订和最终回答；再执行 E-USE 的结构断言。 | S-LEDGER@r26；A-DRAFT、A-INPUTS、A-USE、E-USE 见版本索引。 | locally_verified | **满足当前 Agent 使用过程门槛** |
| U-2 | `YF-USE`：保留实际产物与来源引用，无法确认的内容显式说明。 | A-USE 每个案例的“来源与边界”、跨案例矩阵和 `YF-USE` 映射；E-USE 第二项检查。 | 当前 Agent 将草案事实、风险控制建议、未知系统事实和未执行操作分栏记录；输入、输出和验证报告均以 SHA-256 绑定。 | S-LEDGER@r26；S-RULES@r2；A-DRAFT、A-INPUTS、A-USE、E-USE 见版本索引。 | locally_verified | **满足** |
| M-1 | `YF-MAP`：每条验收有实际证据引用，缺证据时不得宣告完成。 | 本表 G-1 至 D-2 共十行；R-1、R-2、D-1、D-2 明确记录无通过证据。 | 当前 Agent 将 S-LEDGER 的十条原文逐项交叉映射；确定性校验统计十行、六条满足和四条未满足，并禁止把缺证据行写成通过。 | S-LEDGER@r26；S-RULES@r2；本回执由新版配套验证按冻结后 SHA-256 绑定。 | locally_verified | **满足映射内容要求** |
| M-2 | `YF-MAP`：保留实际产物与来源引用，无法确认的内容显式说明。 | “来源版本索引”“执行方式口径”“未决事实与边界”及本表。 | 当前 Agent 记录每行的实际证据、执行方式、来源版本、证据层级和判断；使用 `shasum`、`jq` 与确定性表格断言复核。 | S-GOAL@r1；S-PLAN@r1；S-RULES@r2；S-LEDGER@r26；各产物版本见索引。 | locally_verified | **满足** |
| R-1 | `YF-REVIEW`：核对证据对应关系与实际使用记录，指出不准确结论。 | 无独立复核证据；`deliverables/yf-independent-review.md` 不存在；AWR `YF-REVIEW` evidence 为空。 | **未执行**；仅由当前执行者检查文件缺失和 AWR 状态并修订自己的结论，不能自签为“不同参与者”。 | S-PLAN@r1；S-RULES@r2；S-LEDGER@r26；D-CONTRACT。 | none | **未满足，不宣告完成** |
| R-2 | `YF-REVIEW`：保留实际产物与来源引用，无法确认的内容显式说明。 | 无独立复核产物，因而无可用于通过本条的复核者来源引用。 | **未执行**；文件缺失检查只能证明当前无产物。 | S-LEDGER@r26；D-CONTRACT。 | none | **未满足，不宣告完成** |
| D-1 | `YF-DELIVER`：处理复核意见并生成可追溯完成回执。 | 无复核意见可处理；`deliverables/yf-delivery.md` 不存在；AWR 显示依赖 `YF-REVIEW` 未完成。 | **未执行**；通过 AWR 依赖查询和文件缺失检查确认阻塞，不生成虚假完成回执。 | S-LEDGER@r26；S-PLAN@r1；D-CONTRACT。 | none | **未满足，不宣告完成** |
| D-2 | `YF-DELIVER`：保留实际产物与来源引用，无法确认的内容显式说明。 | 当前只有预交付材料，没有最终交付产物。 | **未执行**；当前材料只用于记录缺口，不能抵充最终产物。 | S-LEDGER@r26；S-RULES@r2；D-CONTRACT。 | none | **未满足，不宣告完成** |

## 项目硬规则对应

| 硬规则 | 对应落实 |
| --- | --- |
| `RULES.md:3`：未知或缺证据的内容显式保留，不把计划、样例或文件存在当通过。 | R-1 至 D-2 保持未满足；“未决事实与边界”保留客户来源、系统事实和真实终态未知。 |
| `RULES.md:7`：本地检查不能替代实际使用；完成前需要逐条映射和不同执行者复核。 | U-1 使用 A-INPUTS 和完整 Agent 修订链，不使用 A-LOCAL 抵充；十条均已映射；独立复核仍为未完成。 |
| `RULES.md:11`：映射注明执行方式和来源版本；无法证明实际使用的条目保持未完成。 | 本表为每行设置“执行方式”“来源版本”；U-1 引用实际 Agent 过程，R/D 缺证据行保持未满足。 |

## 未决事实与边界

1. A-INPUTS 是项目提供的验收输入，但现有来源不能证明其来自客户现场，因此不得称为客户 UAT。
2. A-DRAFT 没有账号申请入口、正式字段、审核者映射、审核标准或时限。
3. A-DRAFT 没有 CSV 编码、分隔符、表头/schema、大小限制、去重、回滚或成功判定规则。
4. A-DRAFT 没有问题反馈入口、具体联系人、SLA、数据共享政策或正式关闭标准。
5. 当前没有真实账号申请、导入或反馈回执；三条说明产物不能替代真实系统成功证据。
6. 独立复核必须由不同参与者执行；当前执行者只修订自己的材料，不签署 `yf-independent-review.md`。
7. 本轮新增映射验证只能支持 `YF-MAP`；不能据此完成 `YF-REVIEW` 或 `YF-DELIVER`。

## 完成状态判定口径

- `YF-GAPS`、`YF-USE`：已有当前、哈希绑定且 AWR 可按源码 SHA 判定为 current 的 `locally_verified` 证据。
- `YF-MAP`：本回执已按最新规则补齐执行方式、来源版本和十条判断；其 AWR 完成状态必须以本文件冻结后的新版证据登记及 `work.completed` 回执为准。
- `YF-REVIEW`：必须由不同参与者执行，当前证据为零。
- `YF-DELIVER`：依赖 `YF-REVIEW`，当前仍阻塞。
- 项目里程碑：在独立复核和最终交付完成前保持 `in_progress`，最终交付结论保持 No-Go。
