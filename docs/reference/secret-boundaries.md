# 秘密数据边界

SEC-002 使用 `awr-core` 的秘密策略 1。组件合同 `tests/security/payloads/contract.json` 1.3.0 保持 32 个条件，补充此策略的引用。原始来源仍由项目维护者负责；AWR 不改写或删除包含敏感值的源文件。

写入前检查原始文本和解析后的结构。覆盖来源文件及直接解析、Manifest、直接投影与来源配置、提案 patch、所有事件、checkpoint 的 digest/列表、证据元数据和产物元数据。拒绝返回固定 `RuleViolation`，不附带命中的值、键或片段。来源读取失败会保留旧投影并报告非新鲜状态；直接运行态写入被拒绝时不提交记录和事件。

识别范围包括：

- API key、access/refresh/id token、client secret、password/passwd/pwd、authorization，以及中文密码、令牌、密钥等标签后的值；支持常见前缀变量名。
- 显式 `private_prompt` 字段、私有提示词标签和独立的私有 prompt 标题块。
- `.env` 风格的大写变量赋值、`export` 赋值、明确标为 env/environment 的对象、列表或多行块。普通的 `environment: candidate` 业务标签可以使用。
- 具有足够长度的常见 OpenAI、GitHub、Slack 凭据前缀，AWS access key、JWT、Bearer/Basic 凭据、PEM 私钥头，以及含用户名和密码的 URL。

检测时折叠全角 ASCII，移除常见零宽/双向格式字符，并检查 JSON Unicode 转义；结构化 JSON/YAML/TOML 另检查解码后的字段。该策略没有宣称识别任意未标记私有文字、任意编码/加密或压缩内容，也没有宣称覆盖所有 Unicode 同形字符。禁止值的判断不以 `test`、`dummy` 或 `example` 字样豁免。

可以保留安全主题的普通讨论、空值和明确占位符，例如 `[redacted]`、`<redacted>`、`[withheld]`、`***`、`${EXAMPLE_API_KEY}`。实际值应从来源中移除，只留下必要的引用。讨论密码保护、token 预算或 API key 管理不会因这些词本身被删除。

产物在创建受管文件前，读取完整且最多 64 MiB 的快照，完成检查，再写入同一份字节并计算摘要。扫描不使用可能漏掉跨块内容的滑动窗口；本阶段为此使用有明确上限的内存缓冲。显式产物/证据正文读取仍受 16 MiB 限制，并在大小、摘要和秘密检查全部通过后返回。正文为有效 JSON 时也检查解码内容。元数据登记不构成对外部文件正文的验证。

对旧数据的输出保护：

- FTS 策略升级为 3，首次读取时重建旧缓存。摘要检查完整字段后才取短文本；敏感摘要标为 `[redacted]`。身份、关联工作或来源引用含敏感值时，整条搜索文档不进入索引。查询参数也经过检查。
- L0、L1、硬规则/关联事实及 delta 检查实际选中的输出。选中的必需事实包含敏感值时返回 `ContextIncomplete`，不返回看似完整的包、哈希或渲染文本。原本不进入 Context 的正文仍然被排除；例如 delta 只使用 checkpoint 的基线和身份，不返回 digest。
- CLI 的敏感参数在参数解析报错前拒绝；证据、完成输入、分支关闭输入及 Manifest 的结构错误不回显字段内容。CLI 和 MCP 共用安全错误报告；MCP 同时保护结构化结果、文本副本与读取输出。

FTS 重建仅更新派生索引。原有权威数据、不可变事件、SQLite 空闲页和备份不因此被擦除；这不是磁盘擦除或凭据轮换功能。

验证入口：

```sh
cargo build -p awr-cli -p awr-mcp --locked
python3 tests/security/payloads/verify_secrets.py --report .local/secret-boundary-checks.json
```

所有负面用例只在独立临时项目中使用合成值；不把秘密样本注入本项目的运行数据库。共用识别器单元检查、存储/来源/上下文检查和真实 CLI/MCP 传输检查分别留证。完整 32 条件、相关回归、实际项目使用和远端回执仍由 SEC-002 的台账证据绑定；这些检查不计 E4 场景或发布验收。
