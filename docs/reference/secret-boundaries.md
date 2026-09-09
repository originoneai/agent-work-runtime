# 秘密数据边界

当前使用 `awr-core` 的秘密策略 3。组件合同 `tests/security/payloads/contract.json` 1.5.0 保持 32 个条件，覆盖无凭据结构定义、Bearer 普通文字和伪装夹带检查。原始来源仍由项目维护者负责；AWR 不改写或删除包含敏感值的源文件。

写入前检查原始文本和解析后的结构。覆盖来源文件及直接解析、Manifest、直接投影与来源配置、提案 patch、所有事件、checkpoint 的 digest/列表、证据元数据和产物元数据。拒绝返回固定 `RuleViolation`，不附带命中的值、键或片段。来源读取失败会保留旧投影并报告非新鲜状态；直接运行态写入被拒绝时不提交记录和事件。

识别范围包括：

- API key、access/refresh/id token、client secret、password/passwd/pwd、authorization，以及中文密码、令牌、密钥等标签后的值；支持常见前缀变量名。
- 显式 `private_prompt` 字段、私有提示词标签和独立的私有 prompt 标题块。
- `.env` 风格的大写变量赋值、`export` 赋值、明确标为 env/environment 的对象、列表或多行块。普通的 `environment: candidate` 业务标签可以使用。
- 具有足够长度的常见 OpenAI、GitHub、Slack 凭据前缀，AWS access key、JWT、Bearer/Basic 凭据、PEM 私钥头，以及含用户名和密码的 URL。

Basic 候选值需能按标准 Base64 解码，并包含用户名和密码之间的冒号，依据 [RFC 7617 第 2 节](https://www.rfc-editor.org/rfc/rfc7617#section-2)。检测也接受省略填充符的形式，不设置会漏掉短用户名/密码的长度下限。普通的 “basic source-intake” 或 “basic authentication” 不因此被当作凭据；带有 `authorization` 等敏感标签的值仍按标签规则检查。

Bearer 的普通协议讨论（例如 `Bearer authentication`）可以保留。`Authorization:` 等赋值或 Header 中的实际值仍按标签规则拒绝，不依赖长度或复杂度；无标签的 Bearer 候选按不透明值形态识别：至少 32 个字符，或至少 8 个字符且包含数字、编码分隔符或非首字母大小写混合。单独一个普通英文单词无法可靠证明是凭据，因此不再仅因跟在 Bearer 后面就拒绝。任意无标签秘密仍不在完整识别承诺内。

结构定义可保留 `{"token":{"type":"string"}}`、`{"authorization":{"type":"http","scheme":"bearer"}}`，以及同类 YAML 块/流式定义和 Markdown 中的代码示例。检测器检查定义的完整结构；单个 `type` 字段不会使其他字段获得豁免。支持的定义包含基础类型、属性、引用、约束与认证方案字段；未知字段仍保守处理。`default`、`example`、`examples`、`const`、`enum` 中的非空实际值，嵌套凭据或额外 `value` 字段不会因处于定义中而放行。定义片段解析上限为 16 KiB，超出时保留拒绝行为。占位值仍须使用明确脱敏形式。

检测时折叠全角 ASCII，移除常见零宽/双向格式字符，并检查 JSON Unicode 转义；结构化 JSON/YAML/TOML 另检查解码后的字段。该策略没有宣称识别任意未标记私有文字、任意编码/加密或压缩内容，也没有宣称覆盖所有 Unicode 同形字符。禁止值的判断不以 `test`、`dummy` 或 `example` 字样豁免。

可以保留安全主题的普通讨论、空值和明确占位符，例如 `[redacted]`、`<redacted>`、`[withheld]`、`***`、`${EXAMPLE_API_KEY}`。实际值应从来源中移除，只留下必要的引用。讨论密码保护、token 预算或 API key 管理不会因这些词本身被删除。

产物在创建受管文件前，读取完整且最多 64 MiB 的快照，完成检查，再写入同一份字节并计算摘要。扫描不使用可能漏掉跨块内容的滑动窗口；为此使用有明确上限的内存缓冲。显式产物/证据正文读取仍受 16 MiB 限制，并在大小、摘要和秘密检查全部通过后返回。正文为有效 JSON 时也检查解码内容。元数据登记不构成对外部文件正文的验证。

对旧数据的输出保护：

- FTS 策略为 5，首次读取时重建旧缓存，包括旧策略下误删的普通认证说明和结构定义。来源解析配置版本为 2，重索引时重新解析旧适配器投影；来源映射变更同样使缓存失效。摘要检查完整字段后才取短文本；敏感摘要标为 `[redacted]`。身份、关联工作或来源引用含敏感值时，整条搜索文档不进入索引。查询参数也经过检查。
- L0、L1、硬规则/关联事实及 delta 检查实际选中的输出。选中的必需事实包含敏感值时返回 `ContextIncomplete`，不返回看似完整的包、哈希或渲染文本。原本不进入 Context 的正文仍然被排除；例如 delta 只使用 checkpoint 的基线和身份，不返回 digest。
- CLI 的敏感参数在参数解析报错前拒绝；证据、完成输入、分支关闭输入及 Manifest 的结构错误不回显字段内容。CLI 和 MCP 共用安全错误报告；MCP 同时保护结构化结果、文本副本与读取输出。

FTS 重建仅更新派生索引。原有权威数据、不可变事件、SQLite 空闲页和备份不因此被擦除；这不是磁盘擦除或凭据轮换功能。

验证入口：

```sh
cargo build -p awr-cli -p awr-mcp --locked
python3 tests/security/payloads/verify_secrets.py --report .local/secret-boundary-checks.json
```

所有负面用例只在独立临时项目中使用合成值；不把秘密样本注入本项目的运行数据库。共用识别器单元检查、存储/来源/上下文检查和真实 CLI/MCP 传输检查分别留证。完整入口 `tests/security/payloads/verify_all.py` 运行全部 32 个条件并保存本地报告；这些检查不计真实客户端场景或发布验收。
