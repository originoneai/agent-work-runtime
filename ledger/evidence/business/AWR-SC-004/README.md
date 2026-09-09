# AWR-SC-004 星桥活动筹备：公开证据候选包

本目录汇集星桥活动筹备的真实业务过程、产物、两轮追问、接续、独立复核、整改、返检和最终筹备交付。它用于审查“筹备材料是否形成并可继续交接”，不表示场地、设备、人员或成品材料已经现场确认，也不表示活动已经执行。

## 主要产物

- [最终筹备交付](project/deliverables/xq-delivery.md)
- [内容安排](project/deliverables/xingqiao-content.md)
- [现场保障](project/deliverables/xingqiao-venue.md)
- [分工与接续](project/deliverables/xingqiao-ownership.md)
- [协作核对](project/deliverables/xingqiao-collaboration-review.md)
- [独立结果汇总](project/deliverables/xingqiao-independent-results.md)
- [首次独立复核](project/deliverables/xq-independent-review.md)
- [整改返检](project/deliverables/xq-independent-recheck.md)

## 如何审查

- [执行记录](execution-record.json)列出 3 条实际 Codex native 会话、13 个自然业务输入、12 个标准封存阶段和一个自定义 `peer-initial` 阶段。
- [门禁复核](gate-review.json)逐项说明九个已有本地证据的门禁及尚缺的独立提交/远端 SHA。
- [输入审计](control/native-input-audit-final.json)区分 13 个自然业务输入与 3 个固定 host setup envelope。
- [并行过程发布](control/parallel-process-review-publication.json)绑定 `project/review-inputs/parallel-process-evidence/` 中 10 个公开文件；`peer-initial` 没有标准 snapshot，不能当成标准阶段。
- [最终阶段观察](control/final-delivery-observation.json)和[最终独立检查](control/final-delivery-independent-check.json)绑定最终产物、运行态和 126 条完成命令的技术读取范围。
- [技术读取时点检查](control/technical-read-scope-at-time-check.json)保留最终交付前 12 阶段、885 条命令的规则时点证据；最终阶段由独立检查另行覆盖。

## 身份、分支与 Git 边界

三位实际业务参与者以 Codex native task ID 区分。AWR 中的 `agent_id`、provider、model 和逻辑分支标签是客户端写入的运行元数据，不能替代 native 身份绑定。AWR 运行分支提供上下文和状态边界，但共享同一个 Git 工作目录，不构成独立 Git worktree。

业务项目当前 Git HEAD `8f4031f40c096691df03099c2382cbb86112be19` 是初始化提交，不含尚未提交的最终交付物。只有本公开包形成独立提交并验证远端 SHA 后，才能满足远端交付门禁。

## 当前边界

当前门禁记录保持 `completion_claim=false`，E4 credit 为 0。保留的场地、设备、人员、材料和外部回执未知项需要后续现场责任人取得新证据；不得由筹备文档或检查器结果代替。原生 transcript、raw events、host memory/reasoning、AWR 数据库和私有上下文不进入公开包。
