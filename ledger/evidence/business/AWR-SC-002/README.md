# 长任务恢复、手册返工与交付

记录来自 2026-09-09 的实际 Codex 工作暂停、不同原生任务恢复、两轮业务返工和独立复核。先读[当前交付清单 v2](project/deliverables/wh-delivery-v2.md)，再看[已复核手册工作稿](project/deliverables/wanghai-operations-manual-draft-v3.md)、[独立复核](project/deliverables/wh-independent-review.md)及[最终工作交接](project/deliverables/wanghai-final-handoff.md)。

原最终清单有一项工作记录指纹错误。原客户端保留错误版本与原核验的漏检，新增[机器清单](project/deliverables/wh-delivery-package-manifest-v2.json)、[修订经过](project/deliverables/wanghai-delivery-fingerprint-correction-v2.md)和能检出旧错误的核验。当前28个机器条目、29行人读清单、36条本地链接均一致；已复核手册和原回执未被覆盖。

交付范围是可追溯的手册工作稿与责任移交。七项业务缺口均未完全关闭；真实运营、业务批准和生效版本仍无证据。原查询偏离、命令失败、拒绝的证据及旧指纹错误均保留。

[门槛审查](gate-review.json)先保留待验证的远端门槛；单场提交推送核验后的独立 acceptance.json 才绑定交付 SHA 并支持台账计数。project/ 保存当前文件和回执，history/ 保留原生各阶段版本，control/ 保留有界业务提取和复核。[回执映射](receipt-storage.json)连接历史指纹与公开位置。完整原生上下文、记忆、思考、事件流和运行数据库保持本地私有。run.json 是准备阶段历史，运行源码、业务来源与公开交付分别绑定。
