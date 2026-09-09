# 独立复核后整改返检输入

本目录只登记生产者在旧独立复核 `requires_executor_changes` 之后提交的整改批次，不是新的独立复核结论，也不替代项目根目录权威来源。

返检者先运行 `.work-receipts/verify-zs-remediation-recheck-inputs.py`，核对 `.work-receipts/zs-remediation-recheck-inputs.sha256` 与当前文件完全匹配，再独立检查：

1. ZS-F01：旧 CR-01/CR-02/CR-03 仍各为唯一 `rejected` 记录且 `apply_attempt: null`；新合并提案 `01M225AVE1BFYY0W4T2EKRV88C` 只出现一次、只有一个成功 apply attempt，当前 acceptance 精确包含两条原要求和两条已确认修订。
2. ZS-F02：`ZS-RECOVER` 的完成状态与下一步不再冲突；`ZS-REVIEW` 已重开为 `planned`，`ZS-DELIVER` 在返检完成前保持依赖阻断。
3. ZS-F03：生产者产物明确区分“旧建议未重放（0 次）”与“新修订已生效（1 次）”；流程状态写回不被当作业务修订应用。
4. 未决与失败：CR-03 具体问题事实及其他未知项继续保留，旧中断、无 checkpoint 限制及所有已知失败记录不被删除或改写。

旧独立复核报告 `deliverables/zs-independent-review.md` 必须保留原文。返检应由不同参与者追加或另行形成结论，生产者不代签；返检通过前不得形成 `deliverables/zs-delivery.md`。
