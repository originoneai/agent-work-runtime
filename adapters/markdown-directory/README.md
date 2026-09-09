# Markdown decision directories

`markdown-directory-v1` discovers `.md`/`.markdown` files in a `decisions` source, recursively by default. Set `[sources.options] recursive = false` for one directory level. Hidden entries, non-Markdown files and symlink children are excluded. Missing or unreadable directories fail explicitly.

File sources read the current files. Git directories enumerate an immutable tree and pin child reads to that commit. `source_identity` keeps the configured Git ref in registration keys; source references retain the immutable commit. `DirectoryInventory::diff` reports added, modified and removed identities. A new Git commit changes commit-bound fingerprints even for unchanged blobs.

Each document produces a Decision with original status, normalized status, title, provenance and selected statements:

- YAML front matter can specify `id`, `title`, `status`, `decision`, `rationale`, `affected_keys`, and `paths`.
- A header immediately after the title can specify `Status: accepted` (also `proposed`, `superseded`, `rejected`). Chinese/simplified/traditional metadata aliases include `状态/狀態`, `日期`, `编号/編號`, `标题/標題`, `关联任务/關聯任務`, and `路径/路徑`; YAML front matter accepts the same aliases. Bullet lists, bold labels, backtick values and full-width colons are supported, including `- **状态：** Accepted`. Chinese statuses such as `已采纳`, `已拒绝`, `已取代`, and `草案` are normalized; the original status string remains available. Header task/path lists accept commas or semicolons. Unknown values remain `unknown` with a warning. Conflicting aliases or front-matter/header declarations fail, while equivalent bilingual statuses agree.
- Decision/Decisions/决策/决定 sections supply the statement, including nested subsections. Rationale/Context/理由/背景/理由与影响/原因 supply its rationale. Flat ADRs use introductory prose after header metadata as the decision statement.
- Other sections, such as an Appendix containing a transcript, are omitted from the decision payload. The complete original document remains reachable by source reference. Missing decision text is reported rather than invented. Source parsing does not imply relevance to a particular task; query/context selection is handled by later items.

The adapter is read-only. The unified `reindex` developer example reconciles successful directory scans with previously managed sources, marking removed source rows and their projections inactive while preserving runtime history. If a directory is unavailable, its cached rows remain available for diagnosis and its sources become unavailable. The lower-level `index_directory` example demonstrates discovery and per-file projection.
