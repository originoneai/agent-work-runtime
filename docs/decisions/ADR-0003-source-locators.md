---
affected_keys: ["*"]
---

# ADR-0003：来源配置与 file/Git 定位

Status: accepted
Date: 2026-09-08

Manifest 使用项目名、source-first 和显式来源映射；每项来源只设置 path 或 locator 之一。path 相对 Project Root，也可指向显式 authorized_roots 内的绝对路径。file:// 使用标准本地绝对文件 URL。配置不复制业务正文。

Git locator 定义为 git://<ref>:<project-relative-path>，例如 git://HEAD:docs/GOALS.md。读取时解析为不可变 commit，按 Project Root 在仓库内的前缀定位 blob，不 checkout、不 fetch、不运行 smudge filter。实际 source ref 记录解析后的 commit；Git fingerprint 绑定解析后的 locator 与原始 blob 字节，文件 fingerprint 为原始字节 SHA-256。

初始文件读取遵守路径范围和尺寸上限。广泛的边界审计和异常矩阵在 M8 进行。Adapter 接口包含 discover/fingerprint/parse/project/plan_mutation/apply_mutation，未实现写回的适配器明确返回 MutationUnsupported。

YAML 与 Markdown 的实际领域解析、目录变化索引在后续台账项目实现。当前本项目的推荐来源映射见 examples/source-manifest/project.toml。
