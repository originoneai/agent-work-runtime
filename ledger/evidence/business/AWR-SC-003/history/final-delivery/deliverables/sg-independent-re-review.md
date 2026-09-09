# 松果伙伴接入整改包独立返检记录

> 返检结论：**本次计划范围整改通过（REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE）**。`R-01.1` 至 `R-01.5` 已逐项关闭；`R-02.1` 至 `R-02.3` 的披露充分，但 `R-02` 记录的是已经发生且必须永久保留的技术参考查找范围偏离，本结论不把它追认为合规。接入开放日期尚未确认。
>
> 本结论只确认当前伙伴接入安排正确表达最新来源、两轮修订、对象范围、依赖、时间边界和未决事项。它不是溪桥或其他伙伴的接入批准、资料核验通过、真实业务验收、E4、最终交付或干净范围执行轨迹认定。

## 1. 身份、固定输入与独立性

- 原独立复核者及本次返检者：`/root/restricted_material_client_run`，当前实际参与者类型为 `luna_worker`；本复核者没有参与生产者材料编写。
- 原复核 AWR 会话：`01M21X23BEFAEAYF6RJ97K3CG3`；原报告 [`sg-independent-review.md`](sg-independent-review.md) 的 SHA-256 仍为 `735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7`，结论 `CHANGES_REQUIRED`，未被改写。
- 本次返检 AWR 会话：`01M2202ANNA5WFK3Q9DZ015B7A`；claim `01M2203KGQ2881PWDRPWTH2SX1`；工作项 `SG-REVIEW`。
- 原生产者实际客户端任务 ID：`01a08381-04a8-7871-9372-a3f55c2d8367`。
- 固定整改包位于 [`review-inputs/corrections/review-correction`](../review-inputs/corrections/review-correction/)。发布清单 [`review-correction-publication.json`](../../control/review-correction-publication.json) 的 SHA-256 为 `8aa3ed46770e7cf44503a6b3f8810f1b918290eb4cfa10ca4ae42410e3826b53`。
- 独立校验见 [`re-review-package-verification.json`](../.work-receipts/re-review-package-verification.json)：发布清单中的 103 个文件逐一匹配，89 份 `.work-receipts/` 文件在固定包与当前项目间逐一未变，86 个 JSON 回执均可解析；四份生产者材料及历史封存均与发布指纹一致。固定摘要保留 70 次命令、4 次非零退出和 29 次文件变化的计数；私有原生上下文未发布，本次返检没有读取或推断其内容。

本次返检只读取项目业务来源、公开固定整改包和获授权的发布清单。没有读取 `operator-state`、`execution-record` 或私有原生 transcript，没有修改生产者材料、原复核报告或历史快照，也没有创建最终交付。

## 2. 两轮实际修订与整改前后指纹

实际业务轮次由三个封存 turn record 固定：initial 在 `2026-09-09T00:11:09.807Z` 看到首次“两家”规则；round-1 在 `2026-09-09T00:31:46.493Z` 看到 `RULES.md` revision `2` 将窗口收紧为一家；round-2 在 `2026-09-09T00:46:04.927Z` 使用核清材料确认溪桥进入本轮，并保留字段说明正文、开放日期和执行人等缺口。对应记录为 [`initial`](../review-inputs/initial/turn-record.json)、[`round-1`](../review-inputs/round-1/turn-record.json) 和 [`round-2`](../review-inputs/round-2/turn-record.json)，其 SHA-256 分别为 `1fb09aa010a79015126e73fb522f4454930c880e4366f8e5fcf201b3a0b79899`、`05a439ea261c688004ab22f39699daeef42f57d63d4cb767419552665a168cc4`、`0a0742f1d545de7faaa3a8d7bd2624e0be251d9a9605636392d24280913b5318`。

| 生产者文件 | 整改前/历史 SHA-256 | 固定整改版及返检读取 SHA-256 | 结论 |
| --- | --- | --- | --- |
| [`songguo-source-versions.md`](songguo-source-versions.md) | `b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04`，initial、round-1、round-2 三份封存一致 | `26bac246b439a54551533d6146b0d12045830b539ce5a5deb0b2e1b431eac136` | 已把旧“两家”限定为历史，并形成当前多版本对照。 |
| [`songguo-change-impact.md`](songguo-change-impact.md) | `6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb` | `c1a644f08291eaf7b716063c11a4ab07efd319ad82ed9d08cc4e7cc6505756cb` | 业务结论未追溯改写，历史与当前链接已分开。 |
| [`songguo-revised-plan.md`](songguo-revised-plan.md) | `58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37` | `8ae8430b33e93a07b74e4c3c26b59bfa3c29e956b590d8eec1883766b5083550` | 执行安排保留，补入版本链、偏离披露和返检门槛。 |
| [`sg-review-response.md`](sg-review-response.md) | 整改前不存在 | `ee0f42b76eeef5755fc79d737beba6e9ccdd163f93c954c2a253c97bec191018` | 对 `R-01`、`R-02` 逐项回应，并未代替独立判断。 |

三份历史版本对照仍保持原字节；round-1 影响说明和 round-2 执行计划也保持原封存哈希。生产者没有用整改文件覆盖历史轮次。

## 3. `R-01` 逐项返检

| 项目 | 返检结论 | 独立依据 |
| --- | --- | --- |
| `R-01.1` 四层事实链 | **关闭** | 当前版本对照明确记录“原草案三家 → 首次生效规则两家 → 追加现行规则一家 → 协调会确认溪桥进入本轮”，并另列由这些事实导出的现行执行安排。 |
| `R-01.2` 轮次、来源、哈希、边界和替代 | **关闭** | `H-00`、`H-01`、`C-01`、`C-02` 分别给出实际输入或封存时间、来源路径、完整 SHA-256、适用边界和替代关系；文档明确这些时间不是规则文件的精确编辑时间。`C-03` 的当前计划哈希由整改回应和固定发布清单共同绑定。 |
| `R-01.3` 当前口径 | **关闭** | 当前规则只绑定 `RULES.md` revision `2`、SHA-256 `bbd2f4b5241b1c3fbdd774c9c749464960beaa47ac1d70501f2ddcdae441499a`；对象事实绑定 [`clarification.md`](../materials/clarification.md) SHA-256 `8177205a3f6aa84f02efb5f3ebb28333c4711db302f6400d443608cb38cf1e1e`。当前窗口一家且由溪桥占用；云岭补职责说明、南园补目录，二者不占本轮窗口。 |
| `R-01.4` 更早历史和批准链 | **关闭并保留条件** | `U-07` 继续声明更早受控权威版本、精确变更时间和批准链未提供，未声称完成不存在的文件级历史 diff 或批准审计。 |
| `R-01.5` 交叉引用 | **关闭** | 四份整改材料共 109 个实际本地引用均解析到现存文件；旧口径引用指向明确标注为历史的 `review-inputs/` 封存，当前口径指向根目录版本对照、revision `2` 规则和核清材料。唯一刻意不存在的链接是未来 `sg-delivery.md`，并在计划中明确要求返检通过前保持不存在。 |

对四份生产者材料检查了原阻断中的当前式旧表述；未发现“当前窗口支持两家伙伴”“当前窗口的伙伴资料核对数量是两家”等旧口径继续作为当前结论。出现“两家”时均位于历史、替代或禁止外推语境中。

## 4. `R-02` 逐项返检

原过程记录 [`reference-lookup-process-record.json`](../review-inputs/reference-lookup-process-record.json) 的 SHA-256 仍为 `b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5`，内部 `independent_assessment: pending` 也保持原样。生产者通过引用原独立判断披露分类，没有改写原始记录来制造事后一致。

| 项目 | 返检结论 | 独立依据 |
| --- | --- | --- |
| `R-02.1` 保留原记录 | **确认充分** | 五项命令仍为 `item_46/61/63/64/66`，原输出、命令和记录哈希未变。 |
| `R-02.2` 分类 | **确认充分，偏离永久保留** | initial 的 `AGENTS.md` 明确允许 `docs/integrations/codex.md`，故 `item_46` 当时允许。`item_61`、`item_63`、`item_64`、`item_66` 发生时超出 initial 白名单；round-1 后允许 `README.md` 与 `docs/reference/cli-mcp-contract.md` 不追溯消除早先偏离，`examples/` 与 `crates/` 仍不在允许范围。 |
| `R-02.3` 影响边界 | **确认充分** | 对五份保留输出检索“溪桥、云岭、南园、松果、伙伴接入、source-change、AWR-SC-007”均为零命中。现有证据支持“技术查找可能影响 AWR 命令和证据结构选择，但未见当前业务答案、未发布业务输入或伙伴事实进入输出”；不能进一步推断为没有任何未公开影响，也不能捏造业务污染。 |

`R-02` 因此不是可被“修复后合规化”的阻断项。整改通过只说明最终材料已如实保留该历史偏离及其有限业务影响；原轮次仍不能被称为干净范围的执行，也不能用于申领 E4。

## 5. 当前安排与继续保留的条件

本次返检确认当前安排级结论可进入原客户端的最终交付处理：唯一资料核对窗口由溪桥占用；溪桥须先取得实际字段说明正文或可访问引用再做内容核对；云岭只补联系人职责说明和申请材料；南园只准备并提交目录；溪桥结案释放窗口后，必须刷新 AWR 和当前规则再选下一家，任何伙伴都不自动晋级。

以下事实仍未提供，整改通过没有补齐它们：

- 接入开放日期；所有未来对外材料仍须逐份明确写出“接入开放日期尚未确认”。
- 溪桥字段说明正文、版本、收到时间、指纹、实际核验及返工结果。
- 云岭联系人职责说明和南园资料目录。
- 计划执行负责人、资料联络人和资料核对人的具体身份。
- 有权决策方的伙伴接入批准和真实业务验收记录。
- 更早受控权威版本、精确变更时间和批准链。

## 6. 交回原客户端的决定

原 `R-01` 业务文档阻断已关闭，`R-02` 已被充分且永久披露。原客户端可以据此解除仅由返检等待造成的 `SG-DELIVER` blocker，并按当前来源形成最终交付；它仍须在最终文件中保留本报告的范围边界、开放日期原句、全部未决事实和 `R-02` 过程限制。生产者材料、规则或核清材料的任一哈希若在最终交付前变化，必须重新核对，不能沿用本次通过结论。

本复核者不代写 `sg-delivery.md`，不解除 `SG-DELIVER`、不作伙伴批准，也不把本地 AWR 完成或返检报告计为最终业务交付或 E4。
