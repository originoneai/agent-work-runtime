# 来源变化后的计划修订与交付

这份记录来自 2026-09-09 的真实 Codex 业务执行与不同参与者的独立复核：重新梳理来源、响应两轮自然业务追问、整改旧版本引用、返检后最终交付。

先读[最终伙伴接入安排](project/deliverables/sg-delivery.md)，再看[来源版本说明](project/deliverables/songguo-source-versions.md)、[变更影响](project/deliverables/songguo-change-impact.md)和[执行计划](project/deliverables/songguo-revised-plan.md)。[原复核](project/deliverables/sg-independent-review.md)、[整改回应](project/deliverables/sg-review-response.md)与[独立返检](project/deliverables/sg-independent-re-review.md)完整保留。

交付范围是来源变化后的计划。当前窗口只有一家；材料正文、实际核验、实名负责人、批准、开放日期和更早批准链仍未确认。原技术查找范围偏离保留为过程缺陷，后续整改不追认其合规。

[逐项门槛审查](gate-review.json)将远端门槛保留为待完成。单场交付提交推送并独立验证后，另一个提交中的 acceptance.json 才绑定该交付 SHA 并支持台账计数。

project/ 保留最终业务来源、产物和原始回执，history/ 保留各轮实际历史版本，control/ 保留业务提取、复核与只读取证。[回执存储映射](receipt-storage.json)连接封存哈希与公开文件。原生完整上下文、记忆、思考、完整事件流和运行数据库留在本地私有存档。原始回执字节不因格式要求被改写。

run.json 是准备阶段历史，其中零模型调用与未绑定参与者不描述后来的实际执行。运行二进制源码 SHA、业务 fixture Git SHA 与单场公开交付 SHA 分别绑定；历史绝对路径表示原始观察位置，公开材料按本包目录和映射定位。
