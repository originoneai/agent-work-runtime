# AWR 开发与台账规则 {severity=info scope=project value=*}

## 权威与范围 {severity=hard scope=project value=*}

1. 用户当前指令决定授权与优先级。当前要求是功能优先，安全和专项测试后置。
2. contracts/awr-v1.json 是版本化范围与验收合同；ledger/work-ledger.yaml 是唯一状态台账。
3. docs/design/agent-work-runtime-design.md 是不可静默改写的设计原文快照。修改范围必须更新合同版本、台账和相关决策，不能通过减少分母宣布完成。
4. ledger/README.md 是派生索引。四类材料分别为 GOALS、PLAN、RULES、work-ledger，不在多个文件手工复制进度。
5. Source-first：项目文件权威；AWR DB 保存投影和自身运行态，来源冲突不能由旧 DB 覆盖。

## 排期与最小检查 {severity=hard scope=project value=*}

P0 功能 → P1 接入 → P2 安全与集中测试 → P3 发布。原文“每 Phase 完整 regression”的建议在当前实施排期中收敛为：功能批次执行直接相关的最小行为检查，完整 regression 在 M9 集中执行。安全和测试任务仍属于 V1 必需范围。

首次实现写回时必须同时实现版本/指纹校验；硬规则、验收和事实不得因压缩而丢失；未知状态、冲突及缺证据显式报错。这些要求是功能契约，不因专项审计后置而移除。

不为文档、简单脚手架堆砌测试。对行为变更运行适当的定向检查，根因修复补同类回归。未实现命令不能返回假成功。

## 状态与证据 {severity=hard scope=project value=*}

状态为 planned、ready、claimed、in_progress、blocked、completed、cancelled。ready 需要依赖完成且无 blocker；开始代码工作前认领对应条目。

completed 必须满足本条全部 acceptance、依赖、证据和远端交付要求。台账用 evidence 引用版本化 JSON 记录；格式见 ledger/evidence/README.md。完成数由脚本派生，不手填。

筹备完成不计产品功能完成。功能实现、本地验证、真实环境验证、候选发布、正式发布使用不同证据等级。源码 commit、远端 commit、报告与适用范围必须绑定。校验器只检查结构和证据元数据，不替代实际执行和复核。

## 真实业务验收 {severity=hard scope=project value=*}

合同固定 8 个业务场景及覆盖矩阵。每个场景均需要自然发起、工具/角色执行、真实产物、两轮有业务意义的追问或返工、独立复核、最终交付和可追溯回执。

从实际用户客户端输入自然业务语言，不泄漏内部 ID、预期答案或“只回复 OK”等测试指令。接口和数据库用于只读取证。每个场景有独立 fixture、namespace、工作图、角色和产物；每个完成场景独立提交并核实远端 SHA。执行者与复核者可区分，授权不足时待复核，不自行制造授权。

阻塞、跳过或待复测保留现场和恢复条件，并继续可独立推进的任务；不计完成，也不以平均分抵消硬门槛。

## Git 与会话交接 {severity=hard scope=project value=*}

- 开始时检查 branch、HEAD、工作区及台账，保护他人修改。
- 只 stage 当前任务的精确路径，提交说明对应具体行为与验证。
- 不提交凭据、用户真实私有台账、运行态 DB 或大型日志；公开 fixture 必须脱敏。
- 发布、生产写入等动作按当时授权执行；当前开源仓库筹备不构成任何生产操作授权。
- 收尾更新当前任务、next action、blocker、open loops、证据与交付回执，运行台账校验并更新派生索引。
- 使用者可明确授权继续下一项；新会话依据最新台账接续，不凭历史完成数推断状态。
