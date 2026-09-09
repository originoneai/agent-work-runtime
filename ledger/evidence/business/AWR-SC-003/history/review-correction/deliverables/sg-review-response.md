# 松果伙伴接入独立复核意见回应与复查交接

> 对外状态声明：接入开放日期尚未确认。
>
> 当前状态：本文件逐条回应 [`sg-independent-review.md`](sg-independent-review.md) 中的 `R-01`、`R-02`，并将整改材料交回原独立复核者检查。本轮不提交最终交付，不代表阻断项已经被复核者关闭，也不代表伙伴接入批准、资料核验通过或业务验收通过。

## 1. 原复核记录与回应边界

- 原独立复核者：`/root/restricted_material_client_run`。
- 原 AWR 复核会话：`01M21X23BEFAEAYF6RJ97K3CG3`，工作项 `SG-REVIEW`。
- 原复核记录：[`sg-independent-review.md`](sg-independent-review.md)，SHA-256 `735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7`，结论 `CHANGES_REQUIRED`。
- 本次执行者没有改写原复核记录，也不代替原复核者判定整改通过。
- 本次 AWR 工作位于 `SG-DELIVER` 的“处理复核意见”阶段；最终交付文件 `deliverables/sg-delivery.md` 保持不存在。

## 2. 修改材料和精确哈希

| 材料 | 历史封存版 | 本次整改版 SHA-256 | 本次修改范围 |
| --- | --- | --- | --- |
| [`songguo-source-versions.md`](songguo-source-versions.md) | initial、round-1、round-2 均为 `b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04` | `26bac246b439a54551533d6146b0d12045830b539ce5a5deb0b2e1b431eac136` | 重构为四层版本链；将“两家”限定为历史规则；加入现行一家窗口、溪桥对象、历史封存、过程偏离和未决事项。 |
| [`songguo-change-impact.md`](songguo-change-impact.md) | round-1 原版 `6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb` | `c1a644f08291eaf7b716063c11a4ab07efd319ad82ed9d08cc4e7cc6505756cb` | 只修正版本基线与当前版本对照的交叉引用，补充 `CHANGES_REQUIRED` 和复查门槛；原业务影响结论不追溯改写。 |
| [`songguo-revised-plan.md`](songguo-revised-plan.md) | round-2 原版 `58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37` | `8ae8430b33e93a07b74e4c3c26b59bfa3c29e956b590d8eec1883766b5083550` | 保留执行安排，补充当前版本对照、技术参考查找偏离、逐条回应和原复核者复查门槛。 |

以下证据保持原样：

- 三轮真实业务记录：[`initial/turn-record.json`](../review-inputs/initial/turn-record.json) `1fb09aa010a79015126e73fb522f4454930c880e4366f8e5fcf201b3a0b79899`、[`round-1/turn-record.json`](../review-inputs/round-1/turn-record.json) `05a439ea261c688004ab22f39699daeef42f57d63d4cb767419552665a168cc4`、[`round-2/turn-record.json`](../review-inputs/round-2/turn-record.json) `0a0742f1d545de7faaa3a8d7bd2624e0be251d9a9605636392d24280913b5318`。
- 原技术参考查找过程记录：[`reference-lookup-process-record.json`](../review-inputs/reference-lookup-process-record.json)，`b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5`。
- 原独立复核记录：[`sg-independent-review.md`](sg-independent-review.md)，`735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7`。

## 3. 对 `R-01` 的逐条回应

### `R-01.1` 覆盖完整四层事实链

**复核要求**：记录“原草案三家 → 首次生效规则两家 → 追加规则一家 → 协调会确认溪桥进入本轮”。

**本次回应**：已在 [`songguo-source-versions.md`](songguo-source-versions.md) 的“当前读取结论”和“四层来源事实与现行执行层”中逐层记录四项来源事实，并单列其导出的现行安排：

1. `H-00`：原草案梳理三家材料；
2. `H-01`：首次规则当时允许两家；
3. `C-01`：追加硬规则将窗口缩为一家；
4. `C-02`：协调会确认溪桥进入本轮；
5. `C-03`：现行计划让溪桥占用唯一窗口，云岭、南园仅补件。

**状态**：执行者认为已处理，等待原复核者核验。

### `R-01.2` 标记轮次、来源、哈希、边界和替代关系

**复核要求**：每层明确时间或轮次、来源路径、SHA-256、适用边界和被替代关系。

**本次回应**：版本表记录了 initial、round-1、round-2 的真实用户输入时间和封存时间，并明确声明这些时间不是规则文件精确编辑时间；每层均列出来源路径和完整 SHA-256。`H-01` 被 `C-01` 收紧，`C-02` 只补足对象事实、不修改硬规则，避免把不同证据层混写。

**状态**：执行者认为已处理，等待原复核者核验。

### `R-01.3` 当前口径只绑定 revision 2 和溪桥核清材料

**复核要求**：旧“两家”不得继续作为当前口径。

**本次回应**：三个根目录材料现在统一使用以下当前口径：

- `RULES.md` source revision `2`、SHA-256 `bbd2f4b5241b1c3fbdd774c9c749464960beaa47ac1d70501f2ddcdae441499a`；
- `clarification.md` SHA-256 `8177205a3f6aa84f02efb5f3ebb28333c4711db302f6400d443608cb38cf1e1e`；
- 当前窗口一家，溪桥为本轮对象；云岭、南园不占用本轮窗口。

所有“两家”表述均置于“首次规则（历史）”“历史固定基线”或 `review-inputs/initial` 的明确上下文中。

**状态**：执行者认为已处理，等待原复核者核验。

### `R-01.4` 保留更早权威版本和批准链缺失

**复核要求**：不得伪造不存在的历史文件级审计或批准链。

**本次回应**：版本对照保留 `U-07`：更早受控权威版本、精确变更时间及批准链未提供，因此不能声称完成全部历史文件级 diff 或批准链审计。三轮封存文件继续保留原字节和哈希，不把后续规则追溯写回历史。

**状态**：已继续保留限制，等待原复核者核验披露是否充分。

### `R-01.5` 更新交叉引用

**复核要求**：影响说明和执行计划不能继续把未标识的旧文件作为可独立使用的当前来源。

**本次回应**：

- [`songguo-change-impact.md`](songguo-change-impact.md) 现在把初版链接直接指向 `review-inputs/initial` 并标注“历史”，同时把根目录版本对照标为当前多版本对照；
- [`songguo-revised-plan.md`](songguo-revised-plan.md) 现在把形成时的 revision `62` 快照标为历史形成快照，并把当前版本链指向修订后的 [`songguo-source-versions.md`](songguo-source-versions.md)；
- 两份文件都引用原独立复核、本文和原始技术参考查找过程记录，并明确当前仍待复查。

**状态**：执行者认为已处理，等待原复核者检查链接和新哈希。

## 4. 对 `R-02` 的逐条回应

### `R-02.1` 原过程记录不删除、不改写

[`reference-lookup-process-record.json`](../review-inputs/reference-lookup-process-record.json) 保持 SHA-256 `b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5`，本次没有修改。后续规则扩大部分技术文档白名单，不追溯抹除发生时的范围偏离。

### `R-02.2` 区分允许读取与实际偏离

本次沿用原独立复核者的判断，不重新包装原过程：

- `item_46`：读取 `docs/integrations/codex.md`，initial 当时已允许，不计为偏离；
- `item_61`、`item_63`、`item_64`、`item_66`：发生时超出 initial 范围，继续标记为技术参考查找偏离；
- `item_66` 涉及的 `crates/` 源码在当前规则下仍禁止读取。

原过程 JSON 内部的原字段、原输出和 `independent_assessment: pending` 保持不变；独立分类来自未改写的复核报告，避免通过修改原始记录制造事后一致。

### `R-02.3` 保留影响边界

原独立复核对五份输出检索业务关键词为零命中。当前只保留其证据边界：过程存在范围偏离，但未见这些输出包含当前业务答案、未发布业务输入或伙伴事实；它们可能帮助了 AWR 命令和证据结构选择。该结论不把原轮次洗成干净范围，不能用于申领 E4，也不替代业务来源的独立验证。

**状态**：执行者认为披露已同步到版本对照和执行计划，等待原复核者确认是否充分。

## 5. 现行安排与仍待确认事项

当前业务安排没有因本次文档整改而改变：

1. 唯一资料核对窗口由溪桥占用；先取得实际字段说明正文或可访问引用，再做完整性和字段核对。
2. 云岭只补联系人职责说明和申请材料；南园只准备并提交目录；二者不占用当前窗口。
3. 溪桥结案释放窗口后，刷新 AWR 和现行规则，再从资料齐全候选中选择下一家；不自动晋级。

以下事项仍未确认：开放日期、溪桥字段说明正文及核验结果、云岭职责说明、南园目录、具体执行人、接入批准、真实业务验收、更早权威版本及批准链、本次整改的独立复查结论。

因此：接入开放日期尚未确认；本次整改不改变任何伙伴的批准或验收状态。

## 6. 交回原独立复核者的检查请求

请原独立复核者 `/root/restricted_material_client_run` 对以下内容进行同人复查：

1. 按本文件第 3 节逐条检查 `R-01.1` 至 `R-01.5`，并核对三份整改材料的新 SHA-256；
2. 按第 4 节确认 `R-02` 原记录未变、四项实际偏离仍被披露、`item_46` 没有被误列为偏离，且影响边界没有被夸大；
3. 检查根目录三个材料中没有把旧“两家窗口”写成当前口径；
4. 检查开放日期、实际材料、负责人、批准和真实验收等未决事项仍完整保留；
5. 给出明确复查结论：逐项关闭、继续要求修改，或保留条件。若未明确关闭阻断项，`SG-DELIVER` 不得完成。

当前执行者只声明“整改材料已准备并交回复查”，不声明“复核通过”。最终交付文件本轮保持不存在。
