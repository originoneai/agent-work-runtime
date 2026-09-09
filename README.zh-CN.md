# AWR · Agent Work Runtime

[English](README.md) · [简体中文](README.zh-CN.md) · [Apache-2.0](LICENSE)

**让 Coding Agent 换个会话，也能接着项目干。**

项目做久了，目标、任务、规则和决策散落各处。新会话往往要重新翻资料，仍可能漏掉阻塞和下一步。AWR 读取已有 Markdown/YAML，保存工作检查点，按当前任务生成有 Token 预算的上下文。它提供 Rust CLI 和 MCP 接口，本地编译上下文，无需调用模型。

```text
项目源文件 → AWR 索引与检查点 → 当前任务上下文 → Coding Agent
    ↑                                              │
    └──────── 经审查的修改与进度记录 ────────────────┘
```

## 能解决什么

- **减少重复阅读**：按任务取目标、规则、验收条件、阻塞、依赖和来源，不必每次通读全部资料。
- **跨会话接续**：保存检查点和未完成事项，恢复时检查来源变化，再继续工作。
- **接入已有项目**：发现源文件、预览字段映射；缺目标、方案或台账时，向 Agent 给出补建步骤。
- **保留原有资料**：Markdown/YAML 继续作为权威来源；SQLite 保存索引和运行状态，版本检查拒绝过期写入。

## 实测能省多少

![公开合成样例的 Token 对比](docs/benchmarks/context-tokens.svg)

[公开可复跑的基准](docs/benchmarks/README.md)包含 150 个合成任务，对全部 39 个未完成任务逐一测量，取最大的上下文：

| 读取内容 | 最大 Token 数 | 比全文读取减少 |
| --- | ---: | ---: |
| 全部 Markdown/YAML 资料 | 18,955 | — |
| AWR 渲染后的工作上下文 | 4,999 | **73.6%** |
| AWR 完整 CLI JSON 响应 | 12,723 | **32.9%** |

**676/676 项必要事实核对通过**，涵盖任务、状态、下一步、验收条件、阻塞、硬规则及未解决依赖。

口径：`o200k_base`，工作上下文预算 5,000 Token；基线为逐份全文读取，并非其他产品或优化检索方案。JSON 元数据有额外开销。数字不包含聊天历史、模型输出及 MCP 封装，**不等于完整任务账单降幅，也不证明模型回答质量**。

## 怎么用

先安装 [Rust](https://www.rust-lang.org/tools/install)，从当前源码构建：

```sh
git clone https://github.com/originoneai/agent-work-runtime.git
cd agent-work-runtime
cargo install --locked --path crates/awr-cli
cargo install --locked --path crates/awr-mcp
```

复制示例，体验初始化、查看任务和获取上下文：

```sh
mkdir -p .local
cp -R examples/basic .local/demo
awr --project .local/demo init --manifest project.toml --accept
awr --project .local/demo status
awr --project .local/demo ready
awr --project .local/demo context compile --work EXAMPLE-001 --goal 'goal#demo'
```

最后一条直接输出可阅读的上下文。接程序时加 `--json`，预算只约束其中的 `work_context.rendered_context`。执行前应检查退出状态和上下文完整性。

接入自己的项目：

```sh
awr --project /path/to/project init
awr --project /path/to/project init --accept
awr --project /path/to/project intake inspect
```

先预览，再接受映射。没有明确目标时，初始化可加 `--goal "你希望完成的事情"`。Agent 按诊断补齐目标与任务，随后重新检查；AWR 不会替你编造业务意图。自定义状态或中文字段可用 `--status-map pending=planned`、`--field-map title=事项` 映射。

接入 MCP 时，让客户端启动 `awr-mcp --project /项目绝对路径`。详见 [MCP 配置](crates/awr-mcp/README.md)、[Codex 接入](docs/integrations/codex.md)、[检查点与恢复示例](examples/codex/README.md)、[项目接管指南](docs/TAKEOVER.md)。

AWR 可托管自己启动的命令，并连接受支持的客户端生命周期事件。任意已有进程或客户端私有会话的恢复，需要对应客户端配合。旧的包仓库预览版可能不包含当前源码的全部命令。

## 参与项目

构建与检查步骤见 [CONTRIBUTING.md](CONTRIBUTING.md)。公开仓库保留使用示例和合成测试夹具；内部开发台账、规划和原始执行记录保留在本地。使用 [Apache-2.0](LICENSE) 协议。
