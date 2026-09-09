# 松果伙伴接入来源版本对照

> 当前对外状态：接入开放日期尚未确认。
>
> 文档状态：独立复核 `R-01` 整改版，已更新到当前来源，等待原独立复核者复查。本文不是最终交付，不代表伙伴接入批准、资料核验通过或业务验收通过。

## 1. 当前读取结论

- **历史规则**：首次生效规则允许两家伙伴进入资料核对窗口，只用于解释当时的安排和修订过程。
- **现行规则**：当前窗口一家；追加硬规则要求资料齐全的申请优先，当前所有执行和验收只按一家窗口处理。
- **现行安排**：协调会已确认溪桥进入本轮唯一核对窗口；云岭补联系人职责说明，南园先提交资料目录，两者均不占用本轮核对名额。
- **仍待确认**：开放日期、溪桥字段说明正文及实际核验结果、三方执行人的具体身份、伙伴接入批准和真实业务验收均没有充分证据。

旧“两家窗口”不是现行口径。任何需要引用旧结论的地方，必须直接引用 `review-inputs/` 中的封存快照并显式标注“历史”；根目录本文件是当前可使用的多版本对照。

明确版本链为：**原草案三家 → 首次生效规则两家 → 追加现行规则一家 → 协调会确认溪桥进入本轮 → 现行执行安排**。

## 2. 四层来源事实与现行执行层

下表中的时间是各轮真实用户输入或封存时间，不声称是规则文件的精确编辑时间。来源及哈希用于固定当轮所见事实。

| 层级 | 轮次和时间 | 来源及 SHA-256 | 当轮事实 | 当前适用边界和替代关系 |
| --- | --- | --- | --- | --- |
| `H-00` 原草案 | initial 轮输入：`2026-09-09T00:11:09.807Z`；封存：`2026-09-09T08:29:36+08:00` | [`initial-plan.md`](../materials/initial-plan.md)，`068d12832d34cef9d11640de05597523bc2647260fd063a863034e54ae0b2513`；[`initial/turn-record.json`](../review-inputs/initial/turn-record.json)，`1fb09aa010a79015126e73fb522f4454930c880e4366f8e5fcf201b3a0b79899` | 原草案希望同时梳理溪桥、云岭、南园三家材料。 | 这是业务草案和材料盘点范围，不是窗口授权，也不能证明三家获准核对。 |
| `H-01` 首次生效规则 | initial 轮所见；同轮封存 | [`initial/sources/RULES.md`](../review-inputs/initial/sources/RULES.md)，AWR source revision `1`，`29d58166a2bf2fcc7420556341b736b7ed896b31af10d0f69a4ca7d5930729c9` | 当时窗口支持两家伙伴资料核对；开放日期未确认。 | **历史规则**。其“两家”数量已被 `C-01` 明确收紧，不能继续称为当前窗口。日期未确认这一边界仍被后续规则继承并加强。 |
| `C-01` 追加现行规则 | round-1 输入：`2026-09-09T00:31:46.493Z`；封存：`2026-09-09T08:44:18+08:00` | 当前 [`RULES.md`](../RULES.md) 及 [`round-1/sources/RULES.md`](../review-inputs/round-1/sources/RULES.md)，AWR source revision `2`，`bbd2f4b5241b1c3fbdd774c9c749464960beaa47ac1d70501f2ddcdae441499a`；[`round-1/turn-record.json`](../review-inputs/round-1/turn-record.json)，`05a439ea261c688004ab22f39699daeef42f57d63d4cb767419552665a168cc4` | 窗口缩为一家；资料齐全优先；每份对外材料都必须明确说明开放日期尚未确认。 | **现行硬规则**。它覆盖 `H-01` 的窗口数量，但保留 `H-01` 作为历史修订证据。 |
| `C-02` 对象核清 | round-2 输入：`2026-09-09T00:46:04.927Z`；封存：`2026-09-09T09:10:21+08:00` | [`clarification.md`](../materials/clarification.md)，`8177205a3f6aa84f02efb5f3ebb28333c4711db302f6400d443608cb38cf1e1e`；[`partner-requests.csv`](../materials/partner-requests.csv)，`af1cd4a95205845f01b56e9ff182d1f5cddbe06b47f0e1b9bb64001a5cf38db3`；[`round-2/turn-record.json`](../review-inputs/round-2/turn-record.json)，`0a0742f1d545de7faaa3a8d7bd2624e0be251d9a9605636392d24280913b5318` | 协调会只确认溪桥进入本轮；溪桥已补交字段说明，云岭职责说明仍缺；南园仍未提交目录；开放日期未确认。 | 这是当前对象和材料状态的事实输入，不修改 `C-01` 的硬规则。它只证明“进入核对”，不证明批准或核验通过。 |
| `C-03` 现行执行安排 | round-2 形成，复核整改时继续沿用 | [`songguo-revised-plan.md`](songguo-revised-plan.md)；其 round-2 封存版见 [`round-2/deliverables/songguo-revised-plan.md`](../review-inputs/round-2/deliverables/songguo-revised-plan.md)，SHA-256 `58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37` | 溪桥占用唯一核对窗口；云岭和南园只并行补件；溪桥结案后刷新规则再决定下一家。 | **现行安排**，但仍待本次整改被原独立复核者复查；不构成最终交付。 |

## 3. 当前权威来源快照

本次整改在 AWR 项目 revision `102`、`SG-DELIVER` revision `2` 的上下文中编制；context hash 为 `0cc98f3826f2abf3572ee31f52c9f9d80d8ae98734b8ffe83d50e7273989802b`。Git 基线仍为 `146ed3bbf79fd0561158e46bb2219eeb05c89e67`，但当前工作树包含尚未提交的来源、AWR 状态和交付材料，因此该提交不能单独重建本文件。

| 领域 | 当前来源 | AWR source revision | SHA-256 | 当前作用 |
| --- | --- | ---: | --- | --- |
| 权威映射 | [`project.toml`](../project.toml) | 不适用 | `e65e64ec8d1f7582698e85d862d56f0ec78402b65601cc388baba3a21fc513f8` | 定义 goal、plan、rules、ledger 四类 AWR 主来源。 |
| 目标 | [`GOALS.md`](../GOALS.md) | `1` | `72db42bd7c8026e45523101a06a3bc3c4f23ab7fd92fbbbe745cc2e15b21a259` | 规则与窗口变化后重新确定执行计划与验收依据。 |
| 计划 | [`PLAN.md`](../PLAN.md) | `1` | `a5fdfcd2e2922d451655eafc43705a8b0ac21baf3587fec73bce49caa15972b5` | 先核对资料和依赖；交付前由不同参与者独立复核。 |
| 规则 | [`RULES.md`](../RULES.md) | `2` | `bbd2f4b5241b1c3fbdd774c9c749464960beaa47ac1d70501f2ddcdae441499a` | 同时保留历史“两家”条款和追加“一家”修订；当前数量按追加规则的一家执行。 |
| 台账 | [`work-ledger.yaml`](../work-ledger.yaml) | `10` | `217b557e5f6479a6a89b3cb0c0bae6980a70b97ca83f38e729634108202571a6` | 本次整改开始时 `SG-DELIVER` 为 `in_progress`；本轮只处理复核意见并请求复查，不完成最终交付。 |

`clarification.md` 和 `partner-requests.csv` 不在 `project.toml` 的 AWR 主来源映射中。它们作为对象和材料状态的项目事实输入使用，每次执行前必须复核文件指纹；若变化，需要重新核清而不能静默沿用。

## 4. 原修订记录的保留方式

根目录文件可继续修订；已经封存到 `review-inputs/` 的历史快照保持原样，不覆盖、不删除、不把后续规则追溯写回旧轮次。

| 记录 | 封存路径与原 SHA-256 | 本次处理 |
| --- | --- | --- |
| 初版版本对照 | [`initial/deliverables/songguo-source-versions.md`](../review-inputs/initial/deliverables/songguo-source-versions.md)，`b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04` | 原样保留，明确标记为只反映首次“两家”规则的历史产物。 |
| round-1 版本对照 | [`round-1/deliverables/songguo-source-versions.md`](../review-inputs/round-1/deliverables/songguo-source-versions.md)，`b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04` | 原样保留；相同哈希证明该轮没有更新版本对照，这正是 `R-01` 的过程事实。 |
| round-2 版本对照 | [`round-2/deliverables/songguo-source-versions.md`](../review-inputs/round-2/deliverables/songguo-source-versions.md)，`b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04` | 原样保留；相同哈希证明对象核清后仍未更新。根目录当前文件由本次整改取代其“当前来源”用途。 |
| 依赖影响说明 | [`round-1/deliverables/songguo-change-impact.md`](../review-inputs/round-1/deliverables/songguo-change-impact.md)，`6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb` | 原样保留；根目录当前文件只更新对版本对照的交叉引用，业务结论不追溯改写。 |
| 执行计划 | [`round-2/deliverables/songguo-revised-plan.md`](../review-inputs/round-2/deliverables/songguo-revised-plan.md)，`58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37` | 原样保留；根目录当前文件补充本次复核整改状态和复查门槛，不把旧轮次写成已通过。 |
| 独立复核 | [`sg-independent-review.md`](sg-independent-review.md)，`735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7` | 原独立复核记录保持不变，结论仍为 `CHANGES_REQUIRED`，等待同一复核者检查本次整改。 |

本次修改后的三个根目录文件精确哈希记录在 [`sg-review-response.md`](sg-review-response.md)，避免在文件内部记录自身哈希造成循环变化。

## 5. 历史规则、现行安排与未决事项

| 主题 | 历史记录 | 现行安排 | 仍待确认或禁止外推 |
| --- | --- | --- | --- |
| 核对容量 | `H-01` 首次规则为两家。 | `C-01` 追加规则为一家。 | 不得把两家重新写成当前容量。 |
| 本轮对象 | 初版没有确认两家具体名单。 | `C-02` 明确溪桥进入本轮唯一窗口。 | 进入核对不等于接入获批或资料通过。 |
| 溪桥 | 初始材料表写目录齐全。 | 已知补交字段说明；执行计划要求先取得正文或可访问引用，再做逐项核对。 | 正文、版本、收到时间、指纹、核验和返工结果仍缺。 |
| 云岭 | 材料表写缺联系人职责说明。 | 只执行职责说明和申请材料补件。 | 未补齐前不能称为资料齐全，也不能占用当前窗口。 |
| 南园 | 材料表写尚未提交目录。 | 先说明准备要求并取得目录。 | 目录提交不等于内容核对完成，也不能自动进入下一轮。 |
| 开放日期 | 从原草案和首次规则起均未确认。 | 继续采用事件门槛，不写具体日期。 | **接入开放日期尚未确认**；所有对外材料必须逐份明示。 |
| 负责人 | 历史材料未给具体姓名。 | 动作启动前在回执或任务记录中实名绑定。 | 未绑定时不能把动作写成已执行；独立复核人必须与执行者不同。 |
| 接入批准与验收 | 历史产物均未提供批准证据。 | 本计划只安排资料核对和补件。 | 文件存在、本地校验、AWR 完成或安排级复核均不等于真实批准、E4 或业务验收。 |

## 6. 依赖与当前流程状态

正式依赖拓扑未变：

`SG-DIFF → SG-DEPEND → SG-PLAN → SG-REVIEW → SG-DELIVER`

- `SG-DIFF`、`SG-DEPEND`、`SG-PLAN` 已形成各自产物；其完成状态不把历史“两家”升级为当前口径。
- `SG-REVIEW` 已形成独立意见，但被审包的 verdict 是 `CHANGES_REQUIRED`。工作项完成只代表意见形成，不代表整改后的包通过。
- 当前 `SG-DELIVER` 仅处于“处理复核意见”的进行中阶段。本轮修订完成后必须交回原独立复核者复查；在其明确关闭 `R-01`、确认 `R-02` 披露充分之前，不创建或提交 `sg-delivery.md`。

## 7. 技术参考查找偏离的永久保留

原始过程记录 [`reference-lookup-process-record.json`](../review-inputs/reference-lookup-process-record.json) 的 SHA-256 为 `b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5`。本次不修改该文件，也不使用后续白名单追溯改写早期行为。

根据原独立复核记录：

- `item_46` 读取 `docs/integrations/codex.md`，在 initial 当时规则下已明确允许，不计为范围偏离。
- `item_61` 搜索父级 `docs/` 与 `examples/`、`item_63` 读取当时尚未允许的 `docs/reference/cli-mcp-contract.md`、`item_64` 搜索 `examples/` 与 `docs/reference/`、`item_66` 读取父级 `README.md` 和 `crates/awr-cli/tests/work_complete_cli.rs`，属于发生时的范围偏离。
- 后续扩大技术文档白名单不能追溯消除已发生的偏离；`crates/` 源码在当前规则下仍不允许读取。
- 独立复核对五份输出检索业务关键词均为零命中，现有证据只支持“过程不合规但未见当前业务答案、未发布业务输入或伙伴事实泄露”，不支持把原执行轨迹称为干净范围，也不能用于申领 E4。

原过程 JSON 内部的字段名称和待评估状态也保持原样；上述分类来自独立复核，不通过改写原始过程记录来制造事后一致。

## 8. 仍待确认事项清单

| ID | 未决事实 | 当前影响 | 解除条件 |
| --- | --- | --- | --- |
| `U-01` | 接入开放日期。 | 禁止任何日期承诺或开放声明。 | 当前权威来源给出可追溯日期并刷新 AWR。 |
| `U-02` | 溪桥字段说明正文、版本、收到时间、指纹及实际核验结果。 | 只能取件登记，不能宣称内容核对完成。 | 实际材料和完整核对、返工、结案证据齐备。 |
| `U-03` | 云岭联系人职责说明。 | 云岭不能成为资料齐全候选。 | 实际说明及来源、版本、收到时间登记完成。 |
| `U-04` | 南园资料目录。 | 南园不能进入内容预检查或候选排序。 | 目录登记并完成预检查。 |
| `U-05` | 执行角色具体姓名。 | 未绑定动作不能记为已执行。 | 在任务记录或回执中实名登记并验证复核隔离。 |
| `U-06` | 接入批准与真实业务验收。 | 禁止“已批准、已开放、已验收”结论。 | 有权决策方提供独立、可追溯记录。 |
| `U-07` | 更早受控权威版本、精确变更时间及批准链。 | 不能声称完成全部历史文件级 diff 或批准链审计。 | 提供受控历史版本或批准记录；未提供时持续披露。 |
| `U-08` | 本次整改是否关闭 `R-01`、`R-02`。 | 阻止最终交付。 | 原独立复核者检查新哈希和逐条回应，给出明确复查结论。 |

## 9. 当前引用规则

- 表述当前规则、窗口和对象时，引用本文件、当前 `RULES.md`、`clarification.md` 和 `songguo-revised-plan.md`。
- 表述首次“两家窗口”时，只引用 `review-inputs/initial` 或其他明确标注为历史的封存路径。
- 表述复核结论时，引用未改写的 `sg-independent-review.md`；本次整改回应见 `sg-review-response.md`。
- 如任一根目录权威来源或材料指纹变化，先刷新 AWR 并重做差异核清，不用本表的旧哈希代替当前读取。

## 10. 来源清单

- 当前业务来源：[`project.toml`](../project.toml)、[`GOALS.md`](../GOALS.md)、[`PLAN.md`](../PLAN.md)、[`RULES.md`](../RULES.md)、[`work-ledger.yaml`](../work-ledger.yaml)
- 当前业务材料：[`initial-plan.md`](../materials/initial-plan.md)、[`partner-requests.csv`](../materials/partner-requests.csv)、[`clarification.md`](../materials/clarification.md)
- 当前执行材料：[`songguo-change-impact.md`](songguo-change-impact.md)、[`songguo-revised-plan.md`](songguo-revised-plan.md)
- 独立复核：[`sg-independent-review.md`](sg-independent-review.md)
- 历史修订封存：[`review-inputs/README.md`](../review-inputs/README.md)、[`initial/turn-record.json`](../review-inputs/initial/turn-record.json)、[`round-1/turn-record.json`](../review-inputs/round-1/turn-record.json)、[`round-2/turn-record.json`](../review-inputs/round-2/turn-record.json)
- 技术参考查找偏离：[`reference-lookup-process-record.json`](../review-inputs/reference-lookup-process-record.json)

以上版本对照用于整改后复查，不是最终交付。接入开放日期尚未确认。
