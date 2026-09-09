# 中断恢复、待应用变更与维护交接

记录来自实际 Codex 客户端中断、不同原生任务恢复、两轮自然业务追问、独立复核整改与最终交付。先读[最终交接](project/deliverables/zs-delivery.md)、[实际最终回复](control/final-delivery-review-extract.json)和[独立返检](project/deliverables/zs-independent-re-review.md)。

三个旧提案均已拒绝且从未应用，确认后的新合并提案应用一次。原 session 未保存 checkpoint，未落盘的信息没有被声称恢复。首次正常退出、后来实际 SIGKILL、自动观察器未触发、失败输入和修订过程分别保留。

当前5/5个工作项完成，权威源的里程碑标记仍为 in_progress。交付范围为恢复与维护交接；归档实施、访问和完整性、具体人员、值班及升级安排、遗留问题事实仍待确认。实际最终回复和完成后回执明确保留这一区别。

[门槛审查](gate-review.json)在产物提交中保留远端门槛待核验；随后独立 acceptance.json 绑定远端 SHA，才支持台账计数。project/ 保存当前产物与回执，history/ 保存阶段快照，control/ 保存有界业务提取与复核。[回执映射](receipt-storage.json)连接历史指纹与公开位置。完整原生上下文、记忆、思考、事件流和运行数据库留在本地私有。
