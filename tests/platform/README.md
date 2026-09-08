# 原生平台验证

[合同](contract.json)定义 AWR-QA-004 的三个目标：macOS arm64、Linux x64 和 Windows x64，均使用 Rust 1.93.1。每个平台需要独立运行 11 道检查；结果以源台账及绑定的实际运行证据为准。工作流存在、交叉编译通过或另一平台通过都不能作为该平台完成。

检查包含全 workspace/all-targets 构建、实际 Rust 测试、doctest、ignored 入口清单与进程终止后的真实 CLI 恢复、全部 8 项 CLI/MCP 对照、手动 checkpoint/resume，以及中文/空格路径、CRLF 来源、WAL/schema 完整性、持久化会话、源码写回回执、过期版本保护、硬规则更新与项目外来源拒绝。最后核对所有已跟踪文件及源码 SHA 没有变化。

从已提交且干净的工作区执行：

```sh
python tests/platform/verify.py --platform macos-arm64 --output .local/platform-local-v1
```

Linux 使用 `linux-x64`，Windows 使用 `windows-x64`。检查器验证实际系统、CPU 架构和 Rust host；不能以参数伪装平台。每次使用新目录，失败结果保留后再修复，避免覆盖现场。脚本使用标准库，设置 Python UTF-8 模式；本地存在 RTK 时经其调用外部命令，GitHub 托管环境直接执行工具。

`.github/workflows/platform.yml` 使用三个独立原生 job，不因一个失败而取消其余结果。GitHub 官方 Actions 固定到已核实的提交，checkout 不保留凭据；工作流仅有 contents:read，上传范围限本次生成的合成 fixture 检查记录，不上传项目运行数据库或用户资料。构建工具链失败时无有效运行报告，该平台仍未验证。

报告保存系统版本、实际架构、编译器、源码/输入/二进制哈希、GitHub run/attempt、每道检查及日志。Rust 数量按各平台实际执行记录；Windows 编译时排除的 Unix 专属测试不计通过。四个 ignored 入口必须有真实父测试或显式恢复命令，列出入口本身不等于执行。

CLI 探针和手动生命周期调用的是 AWR 程序，没有调用 Agent 模型，不计 E4、独立业务复核、性能指标或发布。支持边界仅限合同列出的架构与托管系统版本；UNC/网络盘、旧系统及其他架构仍未验证。

运行器选择依据 [GitHub 官方托管运行器表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。具体运行器镜像版本以每次 job 环境和日志为准，固定 runner 标签不等于固定底层镜像。
