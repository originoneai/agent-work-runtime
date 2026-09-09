# 青禾独立复核问题处理回执与返检包

- 对应独立复核：[青禾项目交接材料独立复核记录](qh-independent-review.md)
- 原复核结论：`requires_executor_changes`
- 原复核产物 SHA-256：`cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1`
- 执行者整改会话：`01M21WPF433PJ678X41F1TNGDS`
- Git source SHA：`cd44a161471cee94fd4f797955327a4fd995cf3a`
- 交接状态：返给独立复核者检查；不是 `QH-DELIVER` 最终交付。
- AWR 返检状态基准：project revision `146`

## 总体处理结论

`QH-R01`、`QH-R02` 的文档和工作记录问题已修订；`QH-R03` 已把安排级与目录级复核边界拆开，但目录、标题和入口仍因缺少真实输入而保持未完成；`QH-R04` 已补充 Unknown locator 投影与显式验证记录的区别，未删除或提升 Unknown 项；`QH-R05` 的原始偏离记录和哈希保持不变。

上述“已处理”只表示执行者已提交整改材料，全部条目仍等待独立复核者返检，本文件不代签返检通过。

## 逐条处理回执

| 编号 | 处理动作 | 产物与回执 | 当前状态 | 返检者检查点 |
| --- | --- | --- | --- | --- |
| `QH-R01` | 重写上下文引用，补齐 QH-INPUT、QH-BRIEF 两阶段、规则修订、独立复核和本轮整改会话；更新当前规则、台账版本、简报指纹与历史演进 | [上下文引用](qinghe-context-reference.md)；`.work-receipts/20260909-QH-REMEDIATION-context-compile.json`；`.work-receipts/20260909-QH-REMEDIATION-context-handoff.json` | 已处理，待独立返检 | 核对会话、revision、context hash、当前来源和历史指纹是否可追溯；不得把本文件视为执行者自证 |
| `QH-R02` | 更新依赖清单，写入当前工作状态、标题核实、联系人清理、域名与具体切换时间、目录级复核和最终交付依赖；通过 AWR 提案修正四个工作项的摘要与下一步 | [依赖清单](qinghe-dependencies.md)；四组 `.work-receipts/20260909-QH-REMEDIATION-ledger-*-{create,submit,approve,apply}.json` | 已处理，待独立返检 | 核对 completed 工作不再写成尚待完成，`QH-REVIEW` 已回到返检状态，`QH-DELIVER` 未完成 |
| `QH-R03` | 未虚构目录草案；把本轮限定为安排级整改返检，单列目录级复核所需真实输入和不能覆盖的结论 | [依赖清单](qinghe-dependencies.md)“返检能覆盖与不能覆盖的范围”；[上下文引用](qinghe-context-reference.md)“本次返检范围” | 边界已澄清；目录级门槛仍未满足 | 确认返检只评价安排与证据边界，不确认实际标题、入口、联系人清理、域名、切换或上线 |
| `QH-R04` | 说明台账 locator-only Unknown 投影与显式 `evidence add` 记录是不同身份和等级；保留 Unknown 缺口，不用 completed 状态掩盖 | [上下文引用](qinghe-context-reference.md)“未验证来源引用与显式验证记录”；`.work-receipts/20260909-QH-REMEDIATION-work-show-qh-input-final.json`、`...-qh-brief-final.json`、`...-qh-review-final.json` | 解释已补，Unknown 项仍显式保留 | 对照 current/locally_verified 记录的完整绑定与 Unknown 记录缺少的四项绑定；确认没有把登记成功写成命令执行或业务验收 |
| `QH-R05` | 原样保留技术参考查找偏离记录；不删除、不改写、不追认合规；本轮仅使用项目允许的 CLI 合同解释证据格式 | `review-inputs/reference-lookup-process-record.json`，SHA-256 `0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920` | 历史已保留；independent assessment 仍为 pending | 复核原文件与哈希未变，并确认该历史不是青禾业务来源、不得计入验收 |

## 工作记录修订回执

| 工作项 | 修订结果 | AWR 应用回执 |
| --- | --- | --- |
| `QH-INPUT` | 保持 completed；明确来源盘点已经完成，缺失材料改列后续移交事项 | `.work-receipts/20260909-QH-REMEDIATION-ledger-QH-INPUT-apply.json` |
| `QH-BRIEF` | 保持 completed；明确最新版简报已经完成，下一步为独立返检 | `.work-receipts/20260909-QH-REMEDIATION-ledger-QH-BRIEF-apply.json` |
| `QH-REVIEW` | 摘要改为已形成安排级复核结论；整改完成后重新开放给不同参与者返检 | `.work-receipts/20260909-QH-REMEDIATION-ledger-QH-REVIEW-apply.json`；`.work-receipts/20260909-QH-REMEDIATION-review-reopen.json` |
| `QH-DELIVER` | 保持 planned；明确本轮只准备返检包，不创建或提交最终交付 | `.work-receipts/20260909-QH-REMEDIATION-ledger-QH-DELIVER-apply.json` |

这些 source mutation proposal 的 submit/approve 是 AWR 字段变更流程回执，actor 均为 `codex-primary`，不构成独立业务复核；独立返检仍须由不同参与者完成。

## 返检包清单

| 文件 | 用途 | SHA-256 / 状态 |
| --- | --- | --- |
| `deliverables/qinghe-brief.md` | 最新安排基准，本轮未修改 | `31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd` |
| `deliverables/qinghe-dependencies.md` | 更新后的状态、依赖、移交项和复核范围 | `42f98e5869ff2edc64b11a51a021b390e1586ea8ad7c1fcf09b1dc0c340385f5` |
| `deliverables/qinghe-context-reference.md` | 当前与历史上下文、来源、证据语义和偏离说明 | `fde028a64afc2d1f8543aa28ee7c90375714aa583ea778b9de7d25e9d7f4124a` |
| `deliverables/qh-independent-review.md` | 原独立复核记录，本轮未修改 | `cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1` |
| `review-inputs/reference-lookup-process-record.json` | 原技术查找偏离历史，本轮未修改 | `0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920` |
| `work-ledger.yaml` | 修正后的工作摘要、下一步和返检状态 | source revision `16`，SHA-256 `4db09cdb71fd2f9b6bf34ded6a9ea9e2fd5bf1567215881efa9c6a0ebe25985c` |

本返检包自身及机器校验结果由 `.work-receipts/20260909-QH-REMEDIATION-evidence-report.json` 绑定；不在本文件内嵌自身哈希，以避免循环引用。

## 明确移交事项

| 移交事项 | 需要的真实输入或确认 | 未取得前状态 |
| --- | --- | --- |
| 三份说明 | 实际文件或链接、版本、访问状态、内容责任信息 | 待提供 |
| 可对外标题 | 对应真实文档或可追溯标题确认 | 未核实，不进入对外目录 |
| 内容目录 | 仅含已核实标题的入口、访问状态及个人联系人清理结果 | 草案不存在，目录级复核未开始 |
| 外部域名 | 信息管理员的可追溯确认 | 待确认 / 未就绪 |
| 旧版页面停用 | 停用日期、确认角色、切换条件和回退安排 | 待确认 |
| 具体切换与上线 | 域名确认、停用条件、目录级复核及适用验收 | 不承诺具体时间，不声明就绪或上线 |

## 交回复核者的检查顺序

1. 以 `RULES.md` r2 和未改动的最新简报为基准，核对依赖清单与上下文引用。
2. 逐条检查 `QH-R01` 至 `QH-R05` 的处理动作与回执，不沿用执行者的“已处理”作为返检结论。
3. 读取当前 AWR `work show --source-sha cd44a161471cee94fd4f797955327a4fd995cf3a`，确认显式记录与 Unknown locator 投影并存且等级没有混淆。
4. 核对 `QH-REVIEW` 可由不同参与者认领返检，`QH-DELIVER` 因返检未完成而不可完成。
5. 确认项目中没有 `deliverables/qh-delivery.md`，本轮没有最终交付声明。

## 本轮证据边界

本轮本地校验只覆盖文件内容、来源引用、AWR 工作记录和历史文件哈希的一致性。它不覆盖三份说明真实性、标题核实、实际目录、联系人清理结果、域名、停用日期、切换时间、外部访问、门户上线、独立返检结论或最终交付。
