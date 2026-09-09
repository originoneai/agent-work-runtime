# 青禾文档门户工作上下文与来源记录

## 当前执行上下文

- 项目：青禾文档门户上线
- 项目标识：`awr-v1/project-onboarding/awr-live-20260909-001`
- Git source SHA：`cd44a161471cee94fd4f797955327a4fd995cf3a`
- 本轮工作范围：处理独立复核 `QH-R01` 至 `QH-R05`，形成返检包；不提交最终交付。
- 本轮 AWR 会话：`01M21WPF433PJ678X41F1TNGDS`
- 初始执行上下文：AWR project revision 109，L1 context hash `7c389acea955d7b2d4c0cd773bedf6f067aef06916d17413dfd3bc6f30bdacee`
- 返检交接上下文：AWR project revision `146`，L1 context hash `d6ec6b01493b6d2da31589646db617b69560854f9f25279bf078107d284510e3`
- 上下文边界：`CONTEXT COMPLETE` 只表示必需的目标、规则、依赖和来源关联已加载；不证明业务任务完成、验证命令实际执行、独立复核通过、目录确认或门户上线。

## 当前权威来源版本

| 来源 | AWR source revision | 指纹 |
| --- | ---: | --- |
| `GOALS.md` | 1 | `sha256:575f650e4ff9b782bebdb8830f5b2670a3bdc447a41ff84e53eec7cf35c249a8` |
| `PLAN.md` | 1 | `sha256:2d726e0e77754a51f779b5eee59ac29c919a4e7ab87b2a43a53a74938b2ba053` |
| `RULES.md` | 2 | `sha256:05c7d1e94f96e3f6bdaccfd8790895a07e11472fe95542c046eb570456ad218d` |
| `work-ledger.yaml` | 16 | `sha256:4db09cdb71fd2f9b6bf34ded6a9ea9e2fd5bf1567215881efa9c6a0ebe25985c` |

`materials/week-window.md` 仍是补充排期材料，不在 `project.toml` 的 AWR 权威来源中；`review-inputs/` 是历史轮次快照和过程记录，也不能覆盖上表中的当前来源。

## 执行与复核上下文沿革

| 阶段 | AWR 会话 | 关键 project revision / context hash | 说明 |
| --- | --- | --- | --- |
| 初始盘点 `QH-INPUT` | `01M21QTD6SSY9DC6S502CKC3CJ` | r20 / `5cbc90dc54185a19ece97e8ff5da4f11f912f97af1670990202f64c2873ddc9c` | 识别三份说明与两项外部确认缺口 |
| 初版简报 `QH-BRIEF` | `01M21S37GKRJVF5Q5VB6DEVP37` | r45 / `8712a6a957cc410716f52432add416610c17f9a8ed9f1fde1677ee372f07f22a`；完成上下文 r53 / `1a04ac88b465bfa1cd06fe25036ecbc4c1692a6acfca93c6e8f556275d38c990` | 形成第一版执行简报 |
| 规则修订 `QH-BRIEF` | `01M21SVQVGGYD76YT3YFHPHH2D` | r77 / `bc9c3b16d3c3c129e5dd1aa2b61c0d7630115418d516e2acac27fb8404d00865`；完成上下文 r85 / `56f09241ae58add90dd7f3cd52115be1deb2f3c1dce4ca37592749fcd90f7542` | 响应 `RULES.md` r2 的标题、联系人和切换时间限制 |
| 安排级独立复核 `QH-REVIEW` | `01M21VK36SJ9KXRB3634VZ1FPS` | r89 / `b93f62ab6796364279983c883f61964e4fcc10d109d04ce2506d660ee0b12445` | 不同参与者形成 `QH-R01` 至 `QH-R05`，结论为 `requires_executor_changes` |
| 本轮整改与返检交接 | `01M21WPF433PJ678X41F1TNGDS` | r109 / `7c389acea955d7b2d4c0cd773bedf6f067aef06916d17413dfd3bc6f30bdacee`；交接值见本页开头 | 更新依赖、上下文、工作记录和逐条回执，将材料交回独立复核者 |

## 关键产物与历史修订

| 对象 | 历史 | 当前或保留状态 |
| --- | --- | --- |
| `deliverables/qinghe-brief.md` | 初版 SHA-256 `4ce7ed36f0f29c2a78c200ee876b4c7da812af5f89cc79387f20a1386274db06` | 规则修订版 SHA-256 `31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd`；本轮不改写 |
| `RULES.md` | r1 SHA-256 `14fb573ef193e475f1f6d6bf54cfcf59888646150cee3ea0719de39733327386` | r2 SHA-256 `05c7d1e94f96e3f6bdaccfd8790895a07e11472fe95542c046eb570456ad218d` |
| `deliverables/qh-independent-review.md` | 无预填版本 | 独立复核 SHA-256 `cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1`；本轮不修改 |
| `review-inputs/reference-lookup-process-record.json` | 技术参考查找偏离的原始过程记录 | SHA-256 `0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920`；原样保留，独立评估仍为 pending |

本轮对 `qinghe-dependencies.md` 和本文件的更新是对复核缺口的修正，不覆盖 `review-inputs/initial`、`round-1`、`round-2` 中的历史快照。

## 未验证来源引用与显式验证记录

| 类型 | AWR 中的表现 | 能说明什么 | 不能说明什么 |
| --- | --- | --- | --- |
| 未验证来源引用 | 从 `work-ledger.yaml` 的 `evidence` locator 投影为类似 `QH-BRIEF/evidence/.work-receipts/...` 的 Unknown 记录；缺少 `sha256`、`source_sha`、`command`、`verified_at` | 只说明权威台账引用了一个报告位置，保留了来源指针 | 不能证明文件内容匹配、验证命令执行、证据仍与当前源码一致，也不能升级为验收通过 |
| 显式验证记录 | 通过 `evidence add` 形成独立 external key，绑定报告哈希、Git source SHA、命令、范围和验证时间；以相同 source SHA 查询时可显示 current / locally_verified | 说明执行者在所声明源码基线上形成并登记了可校验的本地报告 | 登记本身不执行命令，不等于独立复核、真实目录验收、外部环境就绪或最终交付 |

当前显式记录包括 `QH-INPUT-INVENTORY-20260909`、`QH-BRIEF-CHECKLIST-20260909`、`QH-BRIEF-RULES-REVISION-20260909` 和 `QH-INDEPENDENT-REVIEW-20260909`。同一报告 locator 仍可能同时出现 Unknown 投影；两者是不同身份和证据等级，不能互相覆盖、合并或用 completed 状态自动提升。

AWR 公共技术合同进一步说明，`validation_basis=caller_supplied_bindings` 只表示按调用者提交的绑定保存，记录成功不代表命令已执行或业务验收已完成。该解释依据允许使用的技术参考 `/Users/mac/Documents/originone/agent-work-running/docs/reference/cli-mcp-contract.md` 的“记录输入与回执”部分。

## 技术参考查找偏离历史

`review-inputs/reference-lookup-process-record.json` 原样保留了初始执行者曾检索父仓库设计、开发台账、代码与测试的过程。复核记录判断该行为越过当时项目范围，虽未观察到青禾业务答案受到未来提示、评测编号或其他场景成果污染，但这不使该过程合规，也不能把那些越界读取内容列为青禾业务依据。

本轮没有重复访问记录中列出的设计、开发台账、代码或测试；证据格式解释只使用当前 AWR 回执和项目明确允许的 CLI 合同。历史偏离继续保留，不能删除、改写或以本轮说明追认合规。

## 本次返检范围

独立返检可以验证：

- 最新简报、依赖清单、上下文引用和 AWR 工作记录是否一致；
- `QH-R01` 至 `QH-R05` 的处理动作、回执和未决边界；
- Unknown locator 投影与显式验证记录是否被正确解释；
- 技术查找偏离记录是否保持原文件、原哈希和未决评估状态。

独立返检不能验证：

- 三份说明正文、链接、版本、访问状态、可对外标题和责任信息；
- 实际内容目录、入口或个人联系人清理结果；
- 域名接入、旧版停用日期、切换条件、具体切换时间、外部访问就绪或门户上线；
- 最终交付或整个业务场景完成。

## 继续移交的未知事项

1. 安装说明、账号使用说明、常见问题草稿的实际文件或链接、版本、访问状态、可对外标题和责任信息。
2. 只含已核实标题、不展示个人联系人的实际内容目录草案及其目录级复核。
3. 信息管理员对外部域名接入的可追溯确认。
4. 旧版页面停用日期、确认角色、切换条件和回退安排。
5. 具体切换时间、外部访问就绪状态、门户上线状态和最终交付结论。

## 来源

- [最新简报](qinghe-brief.md)。
- [更新后的任务与依赖清单](qinghe-dependencies.md)。
- [独立复核记录](qh-independent-review.md)。
- [项目目标](../GOALS.md)：第 1–3 行。
- [项目计划](../PLAN.md)：第 1–3 行。
- [硬性规则](../RULES.md)：第 1–11 行。
- [工作台账](../work-ledger.yaml)：四个工作项的当前状态、摘要、下一步和证据 locator。
- [技术查找偏离记录](../review-inputs/reference-lookup-process-record.json)：只作过程历史，不是业务来源。
- `/Users/mac/Documents/originone/agent-work-running/docs/reference/cli-mcp-contract.md`：允许使用的 EvidenceDraft 与回执契约。
