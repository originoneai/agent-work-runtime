# 业务场景覆盖矩阵

本表由版本化 fixture 合同生成，只描述材料与覆盖安排。真实执行状态以源台账为准，准备通过不计 E4。

| 场景 | 业务背景 | 工作节点 | 角色 | 覆盖维度 |
| --- | --- | ---: | --- | --- |
| AWR-SC-001 | 青禾文档门户上线 | 4 | executor, reviewer | daily, single_agent, cli |
| AWR-SC-002 | 望海运营手册更新 | 5 | executor, reviewer | daily, recovery, session_change, cli |
| AWR-SC-003 | 松果伙伴接入方案 | 5 | executor, reviewer | boundary, exception, freshness, cli |
| AWR-SC-004 | 星桥活动筹备 | 6 | executor, executor_peer, reviewer | topology, multi_agent, claim_conflict, branch |
| AWR-SC-005 | 云帆帮助中心验收 | 5 | executor, reviewer | daily, boundary, missing_evidence, source_mutation |
| AWR-SC-006 | 竹声维护交接 | 5 | executor, reviewer | exception, recovery, crash, source_mutation |
| AWR-SC-007 | 澄川周报公开版 | 5 | executor, reviewer | permission, boundary, secret, size_limit |
| AWR-SC-008 | 远岚产品发布说明 | 5 | executor, previous_executor, reviewer | channel, topology, recovery, branch_merge, mcp |

所有场景都要求自然发起、实际执行与产物、两轮业务追问、独立复核、最终交付、可追溯回执、独立 fixture，以及独立提交和远端 SHA。

恢复、并行与跨客户端场景还要求真实前序过程；准备工具不会创建会话、claim、checkpoint、事件、通过记录或交付产物。

大型真实项目与性能指标另由 AWR-QA-005 / AWR-P9 系列工作验证；本矩阵不替代该范围。
