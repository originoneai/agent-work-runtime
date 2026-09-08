# 路径边界检查

`contract.json` 定义 AWR-SEC-001 的 26 个组件条件，覆盖允许、拒绝、目录发现、授权根、路径替换及写回恢复。它不是 8 个真实业务场景的替代口径；任务状态以 `ledger/work-ledger.yaml` 为准。

```bash
cargo test -p awr-source --test security_paths --locked
```

当前 `containment.rs` 通过 6 个测试函数执行其中 21 个读取条件。其余 5 个写回 / 恢复条件继续由同一条台账处理，尚不能用读取检查通过宣布 SEC-001 完成。符号链接检查需要对应平台能力；当前执行证据来自 macOS，其他平台的实际验证属于后续平台门槛。

已复现的根因：Locator 解析与目录发现完成授权检查后，只保存路径字符串；再次用普通路径打开文件时，文件或祖先目录已可能被换成符号链接。原始 3 个确定性复现都读到了 fixture 的目录外标记。修复在打开时逐层固定目录句柄，并禁止每一层和最终文件再次跟随符号链接。目录展开也从同样的句柄入口枚举。

显式配置、仍位于授权边界内的符号链接继续先解析到规范目标；显式授权的外部根仍可读取。`open_dir_exact` / `open_file_exact` 只负责安全打开已授权的绝对规范路径；它们不是授权判断器。授权仍由 Locator / Manifest 执行。`read_source_capped` 用于这些来源；`read_capped` 用于调用者明确提供的输入文件，不可替代来源授权流程。

实现使用 [cap-std 的目录句柄](https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html) 和 [cap-fs-ext 的无符号链接目录打开](https://docs.rs/cap-fs-ext/4.0.3/cap_fs_ext/trait.DirExt.html#tymethod.open_dir_nofollow)。依赖版本由 Cargo.lock 固定。多段路径必须逐段处理；仅约束最后一段仍会遗漏父目录替换。
