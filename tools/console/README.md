# AWR Console

AWR 的本地 Web 控制台。看项目状态、挑下一件活、编译上下文包、检查索引有没有过期。

只读（唯一的写操作是「重新索引」），只绑 `127.0.0.1`，零依赖。

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

其他参数：

| 参数 | 作用 |
| --- | --- |
| `--project <路径>` | 要看哪个 AWR 项目，默认当前目录 |
| `--port <端口>` | 换端口，默认 7381 |
| `--demo` | 强制演示模式，不执行任何真实命令 |
| `--no-open` | 不自动打开浏览器 |

---

## 第一次用？

界面第一次打开会自动弹出**新手引导**（5 步，1 分钟）。跳过了也没关系，右上角「新手引导」随时能再看。

三个帮你看懂界面的东西：

- **右上角「术语」** —— work item、queue、checkpoint、budget、drift、revision 这些词的大白话解释。
- **面板标题旁的 `?`** —— 点开是这一块在说什么、该怎么读。
- **每个 `$` 开头的命令框** —— 就是控制台后台真正跑的那条命令，复制到终端跑结果一样。界面不是黑盒。

---

## 四个页面

| 页面 | 回答什么 | 背后的命令 |
| --- | --- | --- |
| 概览 | 现在该干什么 | `awr status` |
| 工作项 | 这件活要做成什么样、卡在哪 | `awr status` + `awr work show KEY` |
| 上下文 | 给 agent 的那包东西里装了什么 | `awr context compile` |
| 索引源 | 控制台看到的还算数吗 | `awr intake inspect` |

**先看哪里：** 概览页的「被阻塞」和「等待中」两个队列，进度停下来的地方都在那儿。

---

## 前置条件

控制台自己不需要装任何东西，但要看真实数据，项目得先初始化过：

```bash
# 装 awr（两个源二选一）
npm install -g @originoneai/agent-work-runtime@0.4.0
# 或 python -m pip install agent-work-runtime==0.4.0

# 初始化项目（先预览，确认了再接受）
awr --project /你的/项目路径 init
awr --project /你的/项目路径 init --accept
```

没初始化过就打开控制台，界面会直接把该跑的命令给你，不会甩一堆报错。

---

## 界面上的数字对不上？

字段映射的来源：

| 命令 | 映射依据 |
| --- | --- |
| `awr status` | 已对照 `crates/awr-runtime/src/status_action.rs` 与 `status_summary.rs` |
| `awr work show` | 已对照 `crates/awr-cli/src/query.rs` |
| `awr context compile` | 已对照 `crates/awr-context/src/{compile,budget}.rs` |
| `awr intake inspect` | **未核对**，仍是按文档推测的 |

前三条读的是源码，但没跑过真实项目验证过；Sources 视图尤其可能对不上。
遇到某一格显示「—」：

1. 展开那个页面底部的 **「原始 JSON」**，看真实字段叫什么。
2. 打开 `public/app.js`，最上面有一张 `FIELD_MAP` 表。
3. 把真实字段名加进对应的候选数组里，刷新页面。

所有字段映射都集中在那一张表里，别处不猜字段。

---

## 设计上的几条硬规则

写在这儿，改代码时别破坏：

- **源文件是唯一的记录来源。** 控制台不编辑 Markdown / YAML，不自己读源文件补数据。
- **数字不自己推算。** 队列的「还有几条」来自 `total` / `omitted`，不是列表长度。
- **不混用两套 blocked。** `awr ready` 的 `blocked_total`（不可选，含已被 claim 的）和 action view 的
  `blocked_count`（真实阻塞）定义不同。界面只用后者。
- **错误原样显示。** AWR 的 `code` 和 message 不重写，只在下面另起一行给建议动作。
- **被规则拦下的敏感内容不回显。** AWR 故意不返回匹配到的原值，控制台也不去读源文件补出来。

---

## 它在仓库里的位置

`tools/console/`，不在 Cargo workspace 里（`Cargo.toml` 的 members 是显式列举的），
所以 `cargo build` 不会碰它，它也不需要 Rust 工具链。

```
server.js            本地桥接：一个 HTTP 请求 = 一条 awr 命令
start.sh             一键启动
public/
  index.html         页面结构
  styles.css         设计令牌与组件（亮/暗双主题）
  app.js             字段映射、渲染、新手引导
  demo-data.js       演示数据（结构与真实 JSON 一致）
```

## 安全

- 只监听 `127.0.0.1`，不监听 `0.0.0.0`。
- 只能执行白名单里的 `awr` 子命令，参数经正则校验，`spawn` 不走 shell。
- 不向任何外部服务发送数据。上下文编译本来就是全本地、不调模型的。
