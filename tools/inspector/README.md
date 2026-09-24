# AWR Inspector

AWR 的本地观察工具。看工作队列、挑下一件活、看清上下文包里装了什么、检查源新鲜度。

**它是什么：** 一个把 AWR 状态显示给人看的界面。
**它不是什么：** 不是 agent 客户端，不是项目管理系统，不编辑权威源文件，不推进工作状态，不替代 CLI 或 MCP。

---

## 三十秒跑起来

只要机器上有 Node 18 以上：

```bash
node server.js
```

浏览器会自动打开 <http://127.0.0.1:7381>。

**没装 `awr` 也能跑。** 这时它进入演示模式，用一份编好的样本项目数据，界面功能完全一样。
先在演示模式里把四个页面点一遍，熟悉了再连自己的项目。

连自己的项目：

```bash
node server.js --project /你的/项目路径
```

| 参数 | 作用 |
| --- | --- |
| `--project <路径>` | 要看哪个 AWR 项目，默认当前目录 |
| `--port <端口>` | 换端口，默认 7381 |
| `--demo` | 强制演示模式，不执行任何真实命令 |
| `--allow-reindex` | 允许从界面触发 `source reindex`，**默认关闭** |
| `--no-open` | 不自动打开浏览器 |

---

## 界面语言

右上角的语言选择器支持 **English / 简体中文**，切换后刷新页面并记住选择。
语言优先级为 URL 的 `?lang=en` 或 `?lang=zh-CN`、上次保存的选择、浏览器语言，最后回退到英文。
静态页面、动态提示、术语、新手引导和演示内容使用同一套本地翻译资源，不加载外部服务。
真实项目的任务标题、来源正文和原始 AWR 错误保持原文；服务端和 CLI 的开发者诊断使用英文。

## 第一次用？

界面第一次打开会自动弹出**新手引导**（5 步，1 分钟）。跳过了也没关系，右上角「新手引导」随时能再看。

三个帮你看懂界面的东西：

- **右上角「术语」** —— work item、queue、checkpoint、budget、drift、revision 这些词的大白话解释。
- **面板标题旁的 `?`** —— 点开是这一块在说什么、该怎么读。
- **每个 `$` 开头的命令框** —— 就是桥接后台真正跑的那条命令，复制到终端跑结果一样。界面不是黑盒。

---

## 四个页面

| 页面 | 回答什么 | 背后的命令 |
| --- | --- | --- |
| 概览 | 现在该干什么 | `awr status`（+ `awr ready` / `awr session list --active` / `awr event history`）|
| 工作项 | 这件活要做成什么样、卡在哪 | `awr work show` |
| 上下文 | 给 agent 的那包东西里装了什么 | `awr context compile` |
| 索引源 | 看到的东西还算数吗 | `awr intake inspect`（读 `organization.sources`）|

**先看哪里：** 概览页的「被阻塞」和「等待中」两个队列，进度停下来的地方都在那儿。

### 每页看得到什么

| 位置 | 观察字段 |
| --- | --- |
| 概览 · 状态条 | 四个队列计数、结构缺口数、组织状态、project revision |
| 上下文 · 这个包有多大 | 必需内容 / 这次装进去的 / 预算上限，都是这次编译实际返回的数。就画在编译按钮下面 |
| 概览 · 结构缺口 | `organization.gaps` 的 code / target / 说明 |
| 概览 · 待查的运行时操作 | `pending_operations`——被中断、结果未知的操作（字段缺失时整块隐藏）|
| 概览 · 活跃 session | `awr session list --active`：agent / status / work / last checkpoint（`Unsupported` 时整块隐藏）|
| 概览 · 最近事件 | `awr event history`：type / summary / importance / 时间（`Unsupported` 时整块隐藏）|
| 工作项 · 表格 | 队列、源状态、负责人、认领状态、诊断码、source revision |
| 工作项 · 详情 | 目标、验收标准、阻塞/等待、依赖与未决依赖、依赖成环、**谁占着这件活**（agent / session / 到期）、**证据与决策**、诊断码 |
| 上下文 · 完整性 | `completeness.status`、**六个维度**（规则 / 目标上下文 / 工作状态 / 验收标准 / 依赖 / 源新鲜度）、**证据缺口**、未决依赖、issues、被省略的块及原因 |
| 索引源 | 每个源的 domain / role / 新鲜度 / revision，与 project revision 并列 |

### Session 与事件

当前源码里 `awr session list --active` 和 `awr event history` 已经实现。概览页会调用它们，显示活跃 session 和最近事件摘要。发布版 0.4.0 若仍返回 `Unsupported`，这两块面板会隐藏，不会用空列表冒充「没有会话」。这里不展开 checkpoint 正文或 open loop；那些仍走 `session show`。

---

## 它到底改不改东西

说清楚这条边界，因为 AWR 是源优先的：

- **不编辑权威源。** 你的 Markdown / YAML 不会被这个工具改一个字。
- **不推进业务状态。** 不做状态流转、不写 evidence、不完成工作项。
- **但读操作会刷新源投影。** `awr status` 会报 `source_refresh_performed: true`，
  `context compile` 会报 `read_only: false`。也就是说它**不是运行时完全只读**，
  AWR 的 SQLite 投影和运行时状态可能被刷新。
- **重新索引是一次显式的维护操作**，默认关闭，要 `--allow-reindex` 才开，界面上还要再确认一次。

需要严格的运行时只读时，用 AWR 自己的 `--cached` 模式（代价是看到的是上次记录的事实，不是最新的）。

---

## 本地边界

绑定 `127.0.0.1` 并不够——浏览器里任何页面都能向回环地址发请求。所以 `/api/*` 还有一层检查：

| 检查 | 挡什么 |
| --- | --- |
| `Host` 必须是预期的回环形式 | DNS rebinding |
| 带了 `Origin` 就必须是本机本端口（`null` 也拒） | 跨站脚本调用 |
| `Sec-Fetch-Site` 为 `cross-site` / `same-site` 时拒绝 | 跨站请求 |
| 非 GET 必须带 `X-AWR-Inspector: 1` | 跨站表单 POST（CSRF）|
| 严格 CSP，无外部资源、无内联脚本 | 页面被注入后外连 |

另外：只能执行白名单里的 `awr` 子命令，参数按字段分别校验，`spawn` 不走 shell，
stdout / stderr / 请求体都有字节上限，并发子进程数有上限。

**不加载任何外部资源。** 用系统字体栈，没有外部字体、没有 CDN。上下文编译本身也是全本地、不调模型的。

---

## 超时的语义

只读命令和写命令处理方式不同：

- **只读命令**超时 60 秒：先 `SIGTERM`，5 秒后 `SIGKILL`，返回 `BridgeTimeout`。重试是安全的。
- **`source reindex`** 超时 120 秒：**不终止子进程**，返回 `OutcomeUnknown`。
  它可能已经生效了。界面不会自动重试，会让你先去查 AWR 的真实状态。
- 写命令的输出超过上限时也**不杀进程**，只丢弃多出来的部分，并同样报
  `OutcomeUnknown`——输出收不全不是终止一个正在改状态的操作的理由。
- 并发上限数的是**活着的子进程**，不是未完成的 HTTP 请求。超时先回响应、
  子进程还在跑时，那个名额仍然被占着，直到它真的退出。

---

## 界面上的数字对不上？

工作项页的「当前队列」表示进行中、可开工、等待中和阻塞队列，不包含已完成或已取消的历史任务。真实项目通过 `/api/work-page?queue=ready&offset=0&limit=10` 分页查询；桥接调用 `awr --json status --queue ready --offset 0 --page-size 10`。默认每页10项，可选20/50/100项，计数与页数据来自同一快照。四个队列均支持分页，超过100项可继续翻页。默认 status/MCP 摘要行为保持不变。查询失败显示错误；切换队列、每页数量时回到第一页，过期请求不会覆盖新页。需要同时更新本地 CLI 和 Inspector；旧 CLI 不支持此新增参数。

四条命令的映射在真实的 `awr 0.4.0` 和 `0.5.0` 上都核对过。

### 版本差异

0.4.0 和 0.5.0 的 `status` 输出**不是同一个形状**，本工具两种都认：

| | 0.4.0 | 0.5.0 及以后 |
| --- | --- | --- |
| `status` 的队列 | 只有 `current` 数组 | `current`/`ready`/`waiting`/`blocked` 四个数组 |
| ready 列表从哪来 | 另跑 `awr ready` | `status` 自带 |
| waiting 队列 | **没有** | 有 |
| 截断条数 | `ready_total` 减列表长度 | `omissions.<队列>` |
| `pending_operations` | 没有 | 有 |

版本里没有的队列，界面显示「—」并说明原因，不拿 0 冒充「没有」；
`pending_operations` 缺失时整块面板隐藏。0.5.0 发布后本工具未改一行代码即适配。

遇到某一格显示「—」：

1. 展开那个页面底部的 **「原始 JSON」**，看真实字段叫什么。
2. 打开 `public/app.js`，最上面有一张 `FIELD_MAP` 表。
3. 把真实字段名加进对应的候选数组里，刷新页面。

所有字段映射都集中在那一张表里，别处不猜字段。

### 一个测不出来的数

界面上没有「不用 AWR 要读多少 token」这种对比条。AWR 不报语料体积，浏览器里也没有
o200k 分词器，这个数造不出来。仓库公开 benchmark 的那组对比（18,955 → 4,998）写在
新手引导第一页，并标明那是 39 个活跃任务上的测量值、不是你项目的数。
你自己项目的实测，在「上下文」页编译一次就有。

---

## 测试

```bash
node --test test/*.test.js
```

48 个用例，零依赖，分三档：

- `test/bridge.test.js` —— 起真实的 `server.js` 子进程、打真实 HTTP 请求，PATH 上放一个
  假 `awr`（`test/fixtures/stub-awr.js`）。覆盖请求来源边界、命令构造、子进程输出、
  超时与生命周期语义、演示模式。
- `test/detail.test.js` —— 在一个最小 DOM 替身（`test/fixtures/dom-stub.js`）上跑
  `app.js` 里**真正的** `renderWorkDetail()`，不是抄一份副本来测。覆盖缓存命中时
  详情与原始响应是否配套、迟到响应（成功与失败）的丢弃、刷新后旧响应的作废。
- `test/packet-size.test.js` —— 同样在替身上跑真正的 `renderPacketSize()` 和
  `doCompile()`。覆盖编译后体积面板会不会填上、三条数字是否原样取自 AWR、
  空态有没有承诺做不到的事、换一次编译旧数字会不会残留。另外三条查页面自身的一致性：
  每个 `?` 都有对应的说明段落（点了没反应的按钮界面上看不出来）、没有打不开的说明、
  主区不再有固定宽度上限。再三条守 `BudgetExceeded` 的处理：重试按钮不超过 AWR 的上限、
  必需量本身超上限时不给必然失败的按钮、先成功再失败时上一次的数字不残留。

CI 见 `.github/workflows/inspector.yml`。

超时相关的用例靠三个只给测试用的环境变量把等待时间压下来：
`AWR_INSPECTOR_READ_TIMEOUT_MS`、`AWR_INSPECTOR_WRITE_TIMEOUT_MS`、
`AWR_INSPECTOR_CONCURRENT`。不设就用默认值。

---

## 它在仓库里的位置

`tools/inspector/`，不在 Cargo workspace 里（`Cargo.toml` 的 members 是显式列举的），
所以 `cargo build` 不会碰它，它也不需要 Rust 工具链。

```
server.js            本地桥接：一个 HTTP 请求 = 一条 awr 命令
start.sh             一键启动
public/
  index.html         页面结构
  styles.css         设计令牌与组件（亮/暗双主题，系统字体）
  app.js             字段映射、渲染、新手引导
  demo-data.js       演示数据（结构与真实 JSON 一致）
test/
  bridge.test.js        桥接测试
  detail.test.js        详情面板的前端回归
  packet-size.test.js   上下文体积面板的回归
  fixtures/stub-awr.js  假的 awr，用来制造边界情况
  fixtures/dom-stub.js  最小 DOM 替身，让 app.js 能在 Node 里跑
```

---

## 设计上的几条硬规则

写在这儿，改代码时别破坏：

- **源文件是唯一的记录来源。** 不编辑 Markdown / YAML，不自己读源文件补数据。
- **数字不自己推算。** 队列的「还有几条」来自 AWR 给的计数，不是列表长度。
- **不混用两套 blocked。** `awr ready` 的 `blocked_total`（不可选，含已被 claim 的）和 action view 的
  `blocked_count`（真实阻塞）定义不同。状态条只用后者。
- **错误原样显示。** AWR 的 `code` 和 message 不重写，只在下面另起一行给建议动作。
- **被规则挡下的敏感内容不回显。** AWR 故意不返回匹配到的原值，本工具也不去读源文件补出来。
