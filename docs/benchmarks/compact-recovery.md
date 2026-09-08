# 会话恢复基准

AWR-P9-002 按[机器合同](../../tests/compact-recovery/contract.json)验证 Session A → 事件和来源变化 → checkpoint → 调用方退出 → Session B → Bootstrap/L1。使用公开的小型模拟项目，四个副本分别覆盖保存前变化、保存后变化、缺失 checkpoint 和规则来源暂时不可用。

```sh
cargo build --workspace
.venv/bin/python tests/compact-recovery/verify.py --awr target/debug/awr --output .local/compact-recovery/run-v1
```

每个命令使用一个新 CLI 进程，副本和原始回执保留在全新的 `.local` 目录；已有目录不能复用。运行报告绑定源码、二进制、合同和 fixture 的哈希，记录每一步命令、退出码及回执哈希。数据库只用于核对会话、claim、checkpoint 等持久记录，不能写数据库来伪造恢复状态。

验证重点是当前工作与验收保持准确、当前来源的 next action 与 checkpoint 保存的 next action 各自保留、open loops 没有丢失、接收者适用的硬规则正确、来源变化可追踪，以及无关已关闭工作的历史没有进入上下文但仍可按需读取。来源在保存前变化时，核对保存的来源观察与 Bootstrap 的 changed entities；保存后变化时，核对恢复后的 delta。

异常必须留下拒绝回执，再检查修复来源或刷新版本后的实际恢复。拒绝时可以刷新来源投影，不能创建半个接续会话、转移 claim 或改写既有 checkpoint。无 checkpoint 的恢复只能保留当前来源及从会话开始的变化，必须明确说明未保存的摘要、next action 和 open loops 无法恢复。

本基准模拟调用方失去内存，不调用模型，不代表真实客户端业务验收或断电恢复。这里显式给 L0 3000-token 定位预算、L1 5000-token 预算，报告实际估算值；V1 的 L0 ≤1000、实际 tokenizer 和压缩比由 AWR-P9-004 验证，不能据此计入指标完成。当前状态与结果以主台账及绑定证据为准。
