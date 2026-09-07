# AWR V1 目标

AWR 为长周期 Agent 工作保存状态、事件和恢复点，并确定性编译当前任务所需的最小充分上下文。

## 交付范围

本轮目标为原始设计 Phase 0–9 覆盖的完整 V1，包括 Source 投影、任务图、运行状态、上下文编译、Compact 恢复、受控 Source 写回、Work Branch、CLI/MCP、基准和发布材料。V0.1 是途中试用里程碑。

原始方案保存在 [design/agent-work-runtime-design.md](design/agent-work-runtime-design.md)，内容未经修改；其 SHA-256 由 [范围合同](../contracts/awr-v1.json) 固定。

## 成功条件

1. Agent 能说明当前目标、工作状态、验收条件、依赖、硬规则、阻塞及下一步。
2. Session、Compact、模型切换或进程退出后，能依据 checkpoint 和来源增量接续工作。
3. 事实可定位到来源和版本；过期、缺失、推断、历史和未验证状态明确可见。
4. 项目原文件持续权威，DB 投影可重建；运行态历史的恢复需要自己的持久化数据。
5. 冻结真实 fixture 上的 Bootstrap <=1,000 tokens，Work Context <=5,000 tokens，压缩比 >20x，必需硬事实召回率 100%。
6. 所有必需功能、完整业务场景、平台验证及基准通过后，才进入 V1 发布。

## 边界

V1 本地优先、单用户，可有多个本地 Agent Session。Rust 单二进制使用 SQLite/WAL/FTS5，以 CLI 和八个 MCP 工具对接 Agent。

Web UI、云同步、账号与团队 RBAC、分布式锁/CRDT、向量/图数据库、数据库内核、自由 Markdown 自动修改及强制 LLM 路径均不在本轮范围。LLM 压缩非关键历史为可选能力，不阻塞 V1。

当前范围和完成数只以合同与台账为准；原始设计中的建议任务被保留并拆细，历史数量不叠加。
