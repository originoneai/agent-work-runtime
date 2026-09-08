# 载荷与秘密数据边界

`contract.json` 定义 AWR-SEC-002 的 32 个组件条件。台账保存任务状态；条件检查不计入 8 个真实业务场景。

当前可执行来源大小检查：

```bash
python3 tests/security/payloads/verify_source_bounds.py --report .local/source-bound-checks.json
```

这一步覆盖 9 个条件：Markdown 2 MiB、YAML 4 MiB 的字节上限，直接解析、刷新、Git、目录来源、提案写回、显式来源读取，以及零值/溢出读取参数。上限值本身允许；超出一个字节即拒绝。较小的调用者读取预算仍然有效。来源变大后保留上一份投影，并显式报告不可用；不把缓存当作新来源。

索引、解析、目录扫描、来源写回和 Doctor 共用这些适配器上限。`source show --content --max-bytes` 可以收紧预算，不能提高适配器上限。解析器也检查已加载快照，避免直接调用绕过限制。来源写回先检查完整输出大小，再建立恢复计划和替换文件。

初始 6 个测试函数在修复前全部失败，确认了超限来源仍能被索引、直接解析或扫描的问题；阶段报告保留原始失败日志摘要。当前阶段不覆盖剩余 23 个事件 Schema、artifact、秘密数据和输出边界条件。`verify_source_bounds.py` 即使通过也明确输出 `item_completed: false`。

秘密数据检查需要拒绝或隐藏具体敏感值，同时保留普通业务文字的可用性。不能通过删除硬规则、验收或事实后宣称 Context 完整；这一部分由后续同一任务的实现与证据验证。
