# 接手新项目并交付当前工作简报

这份记录来自 2026-09-09 的真实 Codex 客户端业务执行：接手项目、按半天窗口整理清单、响应新规则、根据独立复核整改、返检后交付。执行者与两次复核者的实际身份、会话、产物和回执均可追溯。

先读 [最终接手说明](project/deliverables/qh-delivery.md)，再看 [当前简报](project/deliverables/qinghe-brief.md)、[依赖清单](project/deliverables/qinghe-dependencies.md) 和 [来源说明](project/deliverables/qinghe-context-reference.md)。[原独立复核](project/deliverables/qh-independent-review.md)、[整改回应](project/deliverables/qh-review-response.md) 与 [独立返检](project/deliverables/qh-independent-re-review.md) 完整保留。

交付范围是项目接手材料。真实文档、核实标题、目录级复核、域名、停用与切换安排仍明确移交，不能据此声称门户上线。初始技术查找越界依然是历史过程缺陷；后续整改不追认该行为合规。

[逐项门槛审查](gate-review.json) 记录业务链的证据。该文件刻意保留远端提交门槛为待完成；提交推送并独立验证后，另一个提交中的 acceptance.json 才绑定交付 SHA 并支持台账计数，避免自引用提交 SHA。

`project/` 保存最终业务来源、产物和回执；`history/` 保存各实际轮次的来源及产物；`control/` 保存业务提取、复核和只读取证。原生全上下文、记忆、思考、完整事件流与运行数据库留在本地私有存档。工具命令及输出仍逐项保留哈希，业务回执保留原始字节。[回执存储映射](receipt-storage.json) 连接各轮哈希与公开文件。[执行记录](execution-record.json) 连接实际参与者、两轮追问、整改、返检与交付。

run.json 是原始准备阶段记录，其中零模型调用和未绑定参与者只描述当时准备状态，不能用作当前执行统计。运行二进制源码 SHA 与业务 fixture Git SHA 是不同绑定，见 gate-review.json。历史文件里的本机绝对路径和 .local 控制路径用于忠实记录原观察位置；公开包路径由上述目录和映射表定位。
