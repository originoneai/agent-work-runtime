# 青禾项目交接整改独立返检记录

## 返检身份与范围

- 返检日期：2026-09-09
- 实际返检者：`luna_worker`，即原独立复核者；未参与执行者整改产物编写。
- AWR 返检会话：`01M21Z2F3HZ8EHTRYY5N7RMRVZ`
- AWR 认领：`01M21Z2F3JS2KK0S8K77D6MBTT`
- 被审执行者：Codex CLI thread `01a08379-ef89-7173-917d-274662891bcb`
- 执行者源码基线：`cd44a161471cee94fd4f797955327a4fd995cf3a`
- 固定整改快照：`review-inputs/corrections/review-correction/`
- 发布记录：`../control/review-correction-publication.json`，SHA-256 `2b924699054e8f65dd57720567081d8124d060ecc3890b918994493de67fb24e`
- 返检前 AWR：project revision 151，`QH-REVIEW` 为 planned 且无活跃认领；本会话通过正常 AWR 认领并推进为 in_progress。

本次返检只评价接手简报、优先级和依赖、规则变化、来源追溯、证据语义及未决事项移交。它不确认三份说明、目录标题或入口、个人联系人清理结果、外部域名、旧页停用日期、具体切换时间、外部访问、门户上线或最终交付。

## 总体结论

**QH-R01 至 QH-R05 的整改在上述安排级交接范围内通过返检。** 执行者已更新专用依赖清单和上下文引用，修正工作项状态表述，明确拆分安排级与目录级复核，并保留 locator-only Unknown 及初始技术查找范围偏离。该结论允许关闭“独立核对来源、依赖和生效规则并逐条记录要求”的 `QH-REVIEW` 工作，不构成 `QH-DELIVER`、目录级复核、外部就绪或整个业务场景完成。

外部输入仍缺失，因此最终交付只能由原执行者在本返检后继续处理，并继续把无法确认的事项写为未知。项目中仍无 `deliverables/qh-delivery.md`。

## 固定快照与修订指纹

发布记录列出的 136 个快照文件均存在且 SHA-256 全部匹配；完整比对结果见 `.work-receipts/re-review-reviewed-files.json`。关键修订如下：

| 对象 | 整改前 SHA-256 | 固定整改后 SHA-256 | 返检判断 |
| --- | --- | --- | --- |
| `deliverables/qinghe-brief.md` | `31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd` | `31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd` | 未改写；继续正确保留标题、联系人、域名和切换边界 |
| `deliverables/qinghe-dependencies.md` | `b3bbb599f1ef5e3f15ffe1b2e149bb1edc5d218c470eab30186744167071161c` | `42f98e5869ff2edc64b11a51a021b390e1586ea8ad7c1fcf09b1dc0c340385f5` | 已更新现行状态、依赖和移交项 |
| `deliverables/qinghe-context-reference.md` | `8b9508d2965a6d97fd6b76c6afc5f3acad15f52ac326c9819633ad7da19571d3` | `fde028a64afc2d1f8543aa28ee7c90375714aa583ea778b9de7d25e9d7f4124a` | 已补齐两轮简报、规则修订、原复核和整改上下文 |
| `work-ledger.yaml` | `5ca64a6962d2e318dab3c7bc97316eae71a4487248c0d674827b8c2e45ab603a` | `4db09cdb71fd2f9b6bf34ded6a9ea9e2fd5bf1567215881efa9c6a0ebe25985c`（source r16） | 执行者已修正四项摘要与下一步，并重新开放 QH-REVIEW |
| `deliverables/qh-independent-review.md` | `cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1` | 同左 | 原复核原样保留 |
| `review-inputs/reference-lookup-process-record.json` | `0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920` | 同左 | 偏离历史原样保留 |

本返检会话开始后，AWR 正常把 `QH-REVIEW` 从 planned 推进为 in_progress，使当前 `work-ledger.yaml` 成为 source r17、SHA-256 `5d32d35b33c9507a21f9daa9bdecd40a4f9b1b3d96a5f30400dc58bba00e2982`。这属于返检者的实际运行记录；执行者交回时的 r16 文件仍固定在整改快照中。

## QH-R01 至 QH-R05 逐条返检

| 编号 | 独立返检结论 | 核对依据 | 仍保留的限制 |
| --- | --- | --- | --- |
| `QH-R01` | 整改通过 | 新上下文引用列出 QH-INPUT、QH-BRIEF 初版、规则修订、原独立复核和整改会话，绑定 GOALS r1、PLAN r1、RULES r2、整改时 work-ledger r16 及对应指纹；同时说明 `materials/week-window.md` 和 `review-inputs/` 的非权威身份。会话、revision、context hash 与固定 AWR 回执一致。 | 上下文完整只表示必要事实与关联已加载，不证明目录、外部确认或业务验收完成。 |
| `QH-R02` | 整改通过 | 新依赖清单将 QH-INPUT、QH-BRIEF 写为 completed，将 QH-REVIEW 写为待返检、QH-DELIVER 写为 planned；标题核实、联系人清理、域名、旧页停用、目录级复核和最终交付依赖均已明确。四项 source mutation 的 create/submit/approve/apply 与 reopen 回执存在于固定快照。当前 AWR 查询也显示相同依赖关系。 | AWR 字段变更回执只证明工作记录变更；不证明外部材料或确认已经取得。 |
| `QH-R03` | 边界整改通过；目录级门槛仍未满足 | 依赖清单和上下文引用分别列出“安排级返检可以验证”和“不能验证”的范围。执行者没有创建目录草案，也没有把标题、入口、联系人清理、域名、切换或上线写成已确认；`qh-delivery.md` 不存在。 | 三份说明和目录真实输入缺失，后续仍需目录级复核；本条通过只说明边界已正确拆分。 |
| `QH-R04` | 解释整改通过；Unknown 缺口继续保留 | 当前 `work show --source-sha cd44a161...` 显示 QH-INPUT、QH-BRIEF 的显式记录为 current/locally_verified 且绑定完整，同时同 locator 的台账投影仍为 unknown/unknown，缺少 `sha256`、`source_sha`、`command`、`verified_at`。新上下文按公共契约正确解释两者身份不同，未删除或提升 Unknown。 | 显式登记说明调用者提交了绑定完整的本地报告；它不替代命令执行证明、独立业务复核或外部验收。 |
| `QH-R05` | 整改要求通过；历史过程缺陷仍成立 | 原偏离记录的 SHA-256 保持 `0aa782...`，原复核文件也未改写。固定整改回执明确不把父仓库设计、开发台账、代码或测试作为青禾业务来源。本返检认可其“原样保留、不追认合规”的处理。 | 初始范围偏离仍是不合规过程。现有记录只支持“未观察到未来业务提示、评测编号、预期答案或其他场景成果污染”，不能反推当时读取合规。 |

## 验收标准核对

1. **独立核对来源、依赖和生效规则，逐条记录修改要求：满足。** 本返检由与执行者不同的实际 `luna_worker` 会话完成，并逐条判断 QH-R01 至 QH-R05。新增要求仅剩外部输入和最终交付门槛，无需执行者再次改写本次安排级返检包。
2. **保留实际产物与来源引用，无法确认的内容显式说明：满足。** 当前规则、简报、依赖、上下文、原复核、整改回执及历史偏离均有固定哈希；所有无法确认事项继续显式保留。

## 仍待后续处理的风险与移交

以下事项没有可供本返检确认的新材料：

1. 三份说明的实际文件或链接、版本、访问状态、责任信息与可对外标题。
2. 只含已核实标题且不展示个人联系人的实际内容目录，以及目录级独立复核。
3. 信息管理员对外部域名接入的可追溯确认。
4. 旧版页面停用日期、确认角色、切换条件和回退安排。
5. 具体切换时间、外部访问就绪、门户上线与最终交付结论。
6. locator-only Unknown 条目仍应继续显示为证据缺口；后续不得用 completed 状态或显式登记记录覆盖其语义。
7. 初始技术查找范围偏离继续保留为过程缺陷，不计作合规证据。

## 实际返检回执

- 返检前状态：`.work-receipts/re-review-status-before.json`、`re-review-ready-before.json`、`re-review-work-show-before.json`
- 会话与认领：`.work-receipts/re-review-session-start.json`
- 上下文：`.work-receipts/re-review-context-bootstrap.json`、`re-review-context-compile.json`
- 返检进行态：`.work-receipts/re-review-progress.json`、`re-review-status-in-progress.json`
- 工作项实查：`.work-receipts/re-review-work-qh-input.json`、`re-review-work-qh-brief.json`、`re-review-work-qh-review.json`、`re-review-work-qh-deliver.json`
- 文件与固定发布核对：`.work-receipts/re-review-reviewed-files.json`
- 机器校验与绑定：`.work-receipts/re-review-verify.py`、`re-review-verification.json`、`re-review-evidence-draft.json`、`re-review-evidence-add.json`。
- 首次完成失败：`.work-receipts/re-review-work-complete-attempt-1.json`；AWR 因 evidence 与报告的 command 绑定不一致返回 `EvidenceMissing`，未改变工作状态。修正后使用新 evidence key 重新登记，不绕过校验。
- 完成与会话收尾回执将在本文件形成后以 `re-review-*` 前缀固化。

最终交付责任回到原执行者。本返检不生成 `qh-delivery.md`，也不授予 E4 或整个业务场景完成结论。
