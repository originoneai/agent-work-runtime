# 载荷与秘密数据边界

`contract.json` 定义 AWR-SEC-002 的 32 个组件条件。台账保存任务状态；条件检查不计入 8 个真实业务场景。

当前可执行来源大小检查：

```bash
python3 tests/security/payloads/verify_source_bounds.py --report .local/source-bound-checks.json
```

这一步覆盖 9 个条件：Markdown 2 MiB、YAML 4 MiB 的字节上限，直接解析、刷新、Git、目录来源、提案写回、显式来源读取，以及零值/溢出读取参数。上限值本身允许；超出一个字节即拒绝。较小的调用者读取预算仍然有效。来源变大后保留上一份投影，并显式报告不可用；不把缓存当作新来源。

索引、解析、目录扫描、来源写回和 Doctor 共用这些适配器上限。`source show --content --max-bytes` 可以收紧预算，不能提高适配器上限。解析器也检查已加载快照，避免直接调用绕过限制。来源写回先检查完整输出大小，再建立恢复计划和替换文件。

初始 6 个来源测试函数在修复前全部失败，确认了超限来源仍能被索引、直接解析或扫描的问题；来源阶段报告保留原始失败日志摘要。`verify_source_bounds.py` 只检查 9 个来源条件，即使通过也明确输出 `item_completed: false`。

事件阶段另有 5 个条件，覆盖通用和领域载荷、摘要的字节上限，字段白名单、类型、项目内引用和事务回滚。7 个 Rust 测试函数验证存储入口；3 组 CLI/MCP 测试验证字段拒绝、大小边界、发布的 Schema 和有效事件。两组检查覆盖同一组条件，不能相加充当独立条件数。

```bash
cargo build -p awr-cli -p awr-mcp --locked
python3 tests/security/payloads/verify_events.py --report .local/event-bound-checks.json
```

[事件载荷合同](../../../docs/reference/event-payloads.md)列出允许字段和大小。组件合同 1.1.0 明确这些大小参数，保持原有 32 个条件及来源门槛不变。所有基线记录保留各自合同版本；当前版本下的来源和事件检查合计最多覆盖 14/32，另外 18 个 artifact、秘密数据和输出边界条件仍需独立验证。

秘密数据检查需要拒绝或隐藏具体敏感值，同时保留普通业务文字的可用性。不能通过删除硬规则、验收或事实后宣称 Context 完整；这一部分由后续同一任务的实现与证据验证。
