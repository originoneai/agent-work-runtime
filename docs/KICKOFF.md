# 开工入口

## 当前准备范围

本次筹备建立公开仓库、Apache-2.0、原文设计、完整 V1 台账、机器合同、校验器和工作交接入口。运行时功能从下列第一项开始；筹备文档不代表 Rust 工程或 AWR 命令已实现。

以 [台账索引](../ledger/README.md) 查看实时状态，以 [源台账](../ledger/work-ledger.yaml) 查看当前条目。

## 第一项：AWR-P0-001

目标：建立 Cargo Workspace、CLI 入口和核心领域类型。

执行前：
1. 读取 docs/RULES.md、当前合同和本条台账，检查 Git 工作区。
2. 将条目标记 claimed/in_progress 并记录 owner。
3. 重点阅读原方案第 6、10、11、44、45、46 节；检查 Rust 工具链并固定可复现配置。

交付边界：
- awr-core、awr-store、awr-source、awr-context、awr-runtime、awr-cli、awr-mcp 的最小 workspace 边界。
- 共享版本、Apache-2.0 元数据、明确的核心对象类型和错误接口起点。
- awr --help 与 --version 能执行；未实现功能保持显式未支持。
- 先明确接口和依赖方向，避免 crate 循环依赖。

本条完成检查：
- cargo check --workspace。
- awr --help/--version 的真实输出。
- 对新增行为进行必要的定向验证；无须提前实现 M8/M9 的专项套件。
- 交付物、源码 SHA、推送后的远端 SHA 与本条验收绑定。

## 紧接的功能链路

AWR-P0-002 SQLite/迁移 → AWR-P0-003 Schema → AWR-P1-001 Manifest → P1 指纹/适配器/重索引 → P2 任务读取 → P3 运行态 → P4 上下文。

P0-004 领域事务和错误在依赖允许时同步排入。具体选择由依赖就绪状态决定。

## 新会话或中断

先查看 current.work_item、next_action、open_loops，再读取对应任务。不要把计划状态当成实现状态。尚未具备 AWR 自举能力时使用源文件和规划校验器；实现后另以受控任务迁移接入。

真实验收项目、独立复核者和平台测试环境尚待 M9 前落实；它们不阻塞第一项骨架开发。
