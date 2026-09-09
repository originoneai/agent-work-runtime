# 望海交付清单指纹修订记录 v2

- 修订日期：2026-09-09
- AWR 工作项：WH-DELIVER
- 本轮会话：01M227Z6TNNZZ8JAK1A7B5FNDR
- 本轮 claim：01M227ZVTZW7YKR4KWQR4GS48B
- 问题来源：用户复核反馈
- 修订性质：交付清单引用与核验机制修正
- 业务内容边界：不修改已复核手册，不关闭任何业务缺口

## 1. 发现的问题

原交付清单 [wh-delivery.md](wh-delivery.md) 在“本次交付对象”表中声明：

| 对象 | 原声明 SHA-256 | 当前文件实际 SHA-256 | 判定 |
| --- | --- | --- | --- |
| [wanghai-final-work-record.md](wanghai-final-work-record.md) | 36835245017f9a6e6bfb99c02465a4975885c0eeb1806bcd659cfa49813fa0a0 | 3ebbb2b6b37d172a67af13b9982b34721b7b489793e4514651dea0bb64b552de | 不一致 |

原声明值对应工作记录补入首次 WH-DELIVER 完成失败之前的版本。补记失败经过后，工作记录内容和实际指纹发生变化，但原交付清单表格没有同步更新。

原交付清单当前文件指纹为 e29908a4ccd1c9547f009890ff63896c1957e1cba43c714ad08f0a6f5ae52c49。本轮不修改它，以便保留真实错误版本。

## 2. 为什么既有核验没有发现

### 2.1 第一版核验

[WH-DELIVER-final-verification.json](../.work-receipts/WH-DELIVER-final-verification.json) 指纹为 9f0b5c3a130ef1c075b7db3bc5f73be64238bd5afec77f1f41b47791181a4831。它形成时工作记录仍是原声明指纹，随后因 checks 使用对象而不是完成证据契约要求的序列，被 AWR 拒绝用于完成。

该报告、证据登记和失败回执继续保留，不把它改写为成功记录。

### 2.2 第二版核验的漏检

[WH-DELIVER-final-verification-v2.json](../.work-receipts/WH-DELIVER-final-verification-v2.json) 指纹为 61972fed04aba465f8ab105ad4f0b756cd517e1cc6688bf42b7367a7f5181ce0。它正确记录了工作记录实际指纹 3ebbb2b6b37d172a67af13b9982b34721b7b489793e4514651dea0bb64b552de，也正确记录了原交付清单整体指纹，但验证程序只比较“验证报告内的期望值与实际文件”，没有解析原交付清单正文中的路径与 SHA-256 声明。

因此，它能证明两个文件各自未漂移，却不能证明“交付清单对工作记录的引用关系正确”。这是一项真实漏检。原 v2 验证报告、执行脚本、通过记录和原追踪回执均保持不变。

## 3. 本轮修订方式

1. 冻结原 [wh-delivery.md](wh-delivery.md)，保留错误声明。
2. 冻结第一版和第二版核验记录，保留第一次契约失败及第二次关系漏检。
3. 新建 [wh-delivery-package-manifest-v2.json](wh-delivery-package-manifest-v2.json)，为本轮固定交付包逐项记录唯一项目相对路径、类别、角色和实际 SHA-256。
4. 新建 [wh-delivery-v2.md](wh-delivery-v2.md)，使用当前实际指纹，并明确原版与当前版的关系。
5. 新增回归核验：解析 Markdown 清单中每一条本地文件链接及相邻 SHA-256，验证链接目标、项目相对路径、声明指纹、机器清单指纹和实际文件指纹五者一致。
6. 使用原清单作为负向样本。新的核验只有在能够检出原工作记录指纹不一致，同时确认 v2 清单零不一致时才通过。

## 4. 本轮额外过程偏离和失败

本轮继续保留以下真实过程记录：

- 按上级 memory 指引检索 /Users/mac/.codex/memories/MEMORY.md，退出码 1、无命中；仍按项目复核口径记录为读取边界偏离。
- 两次误用未实现的顶级 awr claim 帮助命令，均退出码 1；随后通过 awr work claim 的正式帮助确认正确入口。
- 首次调用 awr work progress 缺少必填 reason，退出码 2；补齐参数后成功推进。
- 指纹关系核验脚本首次执行时把原清单“只有哈希、没有链接”的旧来源表行误判为歧义行并退出 1；随后把负向样本范围收窄为同时含本地链接和指纹的表格行，当前 v2 权威表继续使用严格解析。

机器记录见 [.work-receipts/WH-DELIVER-repair-scope-and-tool-failures.json](../.work-receipts/WH-DELIVER-repair-scope-and-tool-failures.json)。这些失败不会从历史中删除。

## 5. 回归核验成功标准

- 原交付清单被负向检查识别出工作记录声明指纹不一致；
- 当前 v2 交付清单的所有指纹声明均与链接目标实际文件一致；
- 当前 v2 交付清单与机器清单对同一路径给出相同指纹和角色；
- 机器清单中的每个文件存在且实际指纹一致；
- 不允许重复路径、未列入机器清单的带指纹交付行或无法解析到项目内的链接；
- 已复核手册、来源核对、独立复核记录及原历史版本指纹不变；
- GAP-01 至 GAP-07 均继续披露，完全关闭数量仍为 0；
- 结果只可计为资料交付一致性修正，不可计为业务批准、真实操作或 E4。

实际执行结果以本轮新增的指纹对应关系核验报告和 AWR 回执为准，本记录不预填尚未发生的完成事件。
