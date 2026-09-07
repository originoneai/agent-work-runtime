# ADR-0002：Rust 工程与领域接口

Status: accepted
Date: 2026-09-08

固定 Rust 1.93.1 与 edition 2024，Cargo.lock 随源码提交。当前开发机使用 Homebrew 的同版本编译器；rustup 用户可直接使用 rust-toolchain.toml。

七个 crate 按 core → store/source → context/runtime → CLI/MCP 组织。core 不依赖数据库或命令行，保存明确类型、原始状态、来源引用、版本和错误码。store 使用 rusqlite 显式 SQL；Source 解析和 Source 写回不能由 CLI 直接 SQL 绕过。

每项功能依据领域契约实现；本阶段只做编译和必要的直接使用检查，完整测试、优化、打磨在功能链路完成后推进。上下文及工作查询可用后在本项目实际使用，不等待发布阶段。

依赖接口依据维护者文档核对：[clap](https://docs.rs/clap/latest/clap/)、[rusqlite](https://docs.rs/rusqlite/latest/rusqlite/)、[Serde](https://docs.rs/serde/latest/serde/)、[ULID](https://docs.rs/ulid/latest/ulid/)。
