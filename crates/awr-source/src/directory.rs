use crate::{
    Locator, Manifest, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec, locator::git,
    markdown_sections,
};
use awr_core::{Decision, DecisionStatus, EntityKind, Error, ProjectionBatch, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryOptions {
    #[serde(default = "yes")]
    recursive: bool,
}
fn yes() -> bool {
    true
}
fn options(spec: &SourceSpec) -> Result<DirectoryOptions> {
    if spec.domain != "decisions" {
        return Err(Error::InvalidInput(
            "directory adapter requires decisions domain".into(),
        ));
    }
    spec.options
        .clone()
        .try_into()
        .map_err(|e| Error::InvalidInput(format!("directory options: {e}")))
}
fn markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("md") || v.eq_ignore_ascii_case("markdown"))
}
fn visible(path: &Path) -> bool {
    path.components()
        .all(|part| !part.as_os_str().to_string_lossy().starts_with('.'))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryInventory {
    pub files: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectoryDelta {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}
impl DirectoryInventory {
    pub fn diff(&self, previous: &Self) -> DirectoryDelta {
        DirectoryDelta {
            added: self
                .files
                .keys()
                .filter(|key| !previous.files.contains_key(*key))
                .cloned()
                .collect(),
            modified: self
                .files
                .iter()
                .filter(|(key, value)| previous.files.get(*key).is_some_and(|old| old != *value))
                .map(|(key, _)| key.clone())
                .collect(),
            removed: previous
                .files
                .keys()
                .filter(|key| !self.files.contains_key(*key))
                .cloned()
                .collect(),
        }
    }
}

pub struct MarkdownDirectoryAdapter;
impl MarkdownDirectoryAdapter {
    pub fn source_identity(
        &self,
        root: &Path,
        manifest: &Manifest,
        spec: &SourceSpec,
        child: &Locator,
    ) -> Result<String> {
        match (Locator::from_spec(root, manifest, spec)?, child) {
            (Locator::Git { revision, .. }, Locator::Git { path, .. }) => Locator::Git {
                revision,
                path: path.clone(),
            }
            .identity(),
            _ => child.identity(),
        }
    }
    pub fn scan(
        &self,
        root: &Path,
        manifest: &Manifest,
        spec: &SourceSpec,
        cap: u64,
    ) -> Result<DirectoryInventory> {
        let mut inventory = DirectoryInventory::default();
        for locator in self.discover(root, manifest, spec)? {
            inventory.files.insert(
                self.source_identity(root, manifest, spec, &locator)?,
                locator.read(root, cap)?.fingerprint,
            );
        }
        Ok(inventory)
    }
}
impl SourceAdapter for MarkdownDirectoryAdapter {
    fn name(&self) -> &'static str {
        "markdown-directory-v1"
    }
    fn discover(
        &self,
        root: &Path,
        manifest: &Manifest,
        spec: &SourceSpec,
    ) -> Result<Vec<Locator>> {
        let options = options(spec)?;
        let mut files = match Locator::from_spec(root, manifest, spec)? {
            Locator::File(directory) => {
                if !directory.is_dir() {
                    return Err(Error::InvalidInput(
                        "decision source must be a directory".into(),
                    ));
                }
                let mut pending = vec![directory];
                let mut files = vec![];
                while let Some(directory) = pending.pop() {
                    for entry in crate::open_dir_exact(&directory)?
                        .entries()
                        .map_err(|e| Error::SourceUnavailable(e.to_string()))?
                    {
                        let entry = entry.map_err(|e| Error::SourceUnavailable(e.to_string()))?;
                        if entry.file_name().to_string_lossy().starts_with('.') {
                            continue;
                        }
                        let kind = entry
                            .file_type()
                            .map_err(|e| Error::SourceUnavailable(e.to_string()))?;
                        // Symlink children are not implicit new authority roots.
                        if kind.is_dir() && options.recursive {
                            pending.push(directory.join(entry.file_name()));
                        } else if kind.is_file() && markdown_path(Path::new(&entry.file_name())) {
                            files.push(Locator::File(directory.join(entry.file_name())));
                        }
                        if files.len() + pending.len() > 4096 {
                            return Err(Error::BudgetExceeded {
                                required: files.len() + pending.len(),
                                budget: 4096,
                            });
                        }
                    }
                }
                files
            }
            Locator::Git { revision, path } => {
                git_files(root, &revision, &path, options.recursive)?
            }
        };
        files.sort_by_key(|locator| match locator {
            Locator::File(path) => path.to_string_lossy().into_owned(),
            Locator::Git { path, .. } => path.to_string_lossy().into_owned(),
        });
        Ok(files)
    }
    fn parse(
        &self,
        snapshot: &SourceSnapshot,
        context: &ParseContext<'_>,
        spec: &SourceSpec,
    ) -> Result<ProjectionBatch> {
        options(spec)?;
        let text = snapshot.text()?;
        let sections = markdown_sections(snapshot)?;
        let header = sections.iter().find(|section| section.level > 0);
        let metadata = frontmatter(text)?;
        let mut warnings = vec![];
        let header_fields = header
            .map(|h| header_fields(&h.body))
            .transpose()?
            .unwrap_or_default();
        if let (Some(a), Some(b)) = (
            metadata_string(&metadata, "status")?,
            header_fields.get("status"),
        ) {
            if a.trim().to_lowercase() != b.trim().to_lowercase() {
                return Err(Error::SourceConflict(
                    "conflicting ADR status declarations".into(),
                ));
            }
        }
        let raw_status = metadata_string(&metadata, "status")?
            .or_else(|| header_fields.get("status").cloned())
            .unwrap_or_default();
        let status = match raw_status.trim().to_lowercase().as_str() {
            "proposed" => DecisionStatus::Proposed,
            "accepted" => DecisionStatus::Accepted,
            "superseded" => DecisionStatus::Superseded,
            "rejected" => DecisionStatus::Rejected,
            _ => DecisionStatus::Unknown,
        };
        if status == DecisionStatus::Unknown {
            warnings.push(format!("decision status unresolved: {raw_status:?}"));
        }
        let title = metadata_string(&metadata, "title")?
            .or_else(|| header.map(|h| h.title.clone()))
            .unwrap_or_else(|| context.source.locator.clone());
        let body = |names: &[&str]| {
            let mut chosen = vec![];
            let mut index = 0;
            while let Some(section) = sections.get(index) {
                index += 1;
                if !names.contains(&section.title.trim().to_lowercase().as_str()) {
                    continue;
                }
                chosen.push(section.body.clone());
                while let Some(child) = sections
                    .get(index)
                    .filter(|child| child.level > section.level)
                {
                    chosen.push(format!("{}\n\n{}", child.title, child.body));
                    index += 1;
                }
            }
            chosen.join("\n\n")
        };
        let explicit_decision = metadata_string(&metadata, "decision")?
            .unwrap_or_else(|| body(&["decision", "decisions", "决策", "决定"]));
        let decision = if explicit_decision.trim().is_empty() {
            header
                .map(|section| without_header_fields(&section.body))
                .unwrap_or_default()
        } else {
            explicit_decision
        };
        if decision.trim().is_empty() {
            warnings.push("no decision statement identified; consult the source reference".into());
        }
        let rationale = metadata_string(&metadata, "rationale")?.unwrap_or_else(|| {
            body(&["rationale", "context", "理由", "背景", "理由与影响", "原因"])
        });
        let key =
            metadata_string(&metadata, "id")?.unwrap_or_else(|| context.source.locator.clone());
        let lines = text.lines().count();
        if lines == 0 {
            return Err(Error::InvalidInput("empty decision document".into()));
        }
        let mut meta = context.meta(
            EntityKind::Decision,
            &key,
            snapshot,
            Some("document".into()),
            Some((1, lines)),
        )?;
        meta.source_ref.section_fingerprint = Some(snapshot.section_fingerprint(1, lines)?);
        let decision = Decision {
            meta,
            title,
            status,
            raw_status,
            decision,
            rationale,
            affected_keys: metadata_strings(&metadata, "affected_keys")?,
            paths: metadata_strings(&metadata, "paths")?,
        };
        Ok(ProjectionBatch {
            decisions: vec![decision],
            warnings,
            ..Default::default()
        })
    }
}

fn git_files(root: &Path, revision: &str, path: &Path, recursive: bool) -> Result<Vec<Locator>> {
    let commit = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )?;
    let commit = std::str::from_utf8(&commit)
        .map_err(|e| Error::InvalidInput(e.to_string()))?
        .trim();
    let prefix = git(root, &["rev-parse", "--show-prefix"])?;
    let prefix = std::str::from_utf8(&prefix)
        .map_err(|e| Error::InvalidInput(e.to_string()))?
        .trim_end_matches('\n');
    let relative = path
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(value) => Some(
                value
                    .to_str()
                    .ok_or_else(|| Error::InvalidInput("Git path must be UTF-8".into())),
            ),
            _ => None,
        })
        .collect::<Result<Vec<_>>>()?
        .join("/");
    let object = format!("{commit}:{prefix}{relative}");
    if git(root, &["cat-file", "-t", &object])? != b"tree\n" {
        return Err(Error::InvalidInput(
            "Git decision source must be a tree".into(),
        ));
    }
    let args = if recursive {
        vec!["ls-tree", "-r", "-z", &object]
    } else {
        vec!["ls-tree", "-z", &object]
    };
    let listing = git(root, &args)?;
    let mut files = vec![];
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let (entry, name) = record
            .split_once('\t')
            .ok_or_else(|| Error::SourceUnavailable("invalid Git tree entry".into()))?;
        let file = PathBuf::from(name);
        if (entry.starts_with("100644 blob ") || entry.starts_with("100755 blob "))
            && visible(&file)
            && markdown_path(&file)
        {
            // Read the pinned tree we just enumerated. Ref identity is restored by the indexer registration mapping.
            files.push(Locator::Git {
                revision: commit.into(),
                path: path.join(file),
            });
        }
        if files.len() > 4096 {
            return Err(Error::BudgetExceeded {
                required: files.len(),
                budget: 4096,
            });
        }
    }
    Ok(files)
}

fn frontmatter(text: &str) -> Result<Value> {
    let mut lines = text.split_inclusive('\n');
    if lines.next().is_none_or(|line| line.trim() != "---") {
        return Ok(Value::Null);
    }
    let mut body = String::new();
    for line in lines {
        if line.trim() == "---" || line.trim() == "..." {
            let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&body)
                .map_err(|e| Error::InvalidInput(format!("ADR metadata: {e}")))?;
            let value = serde_json::to_value(value)?;
            if !value.is_object() {
                return Err(Error::InvalidInput("ADR metadata must be a mapping".into()));
            }
            return Ok(value);
        }
        body.push_str(line);
    }
    Err(Error::InvalidInput(
        "unterminated ADR metadata block".into(),
    ))
}
fn metadata_string(value: &Value, key: &str) -> Result<Option<String>> {
    match &value[key] {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(Error::InvalidInput(format!("ADR {key} must be a string"))),
    }
}
fn metadata_strings(value: &Value, key: &str) -> Result<Vec<String>> {
    match &value[key] {
        Value::Null => Ok(vec![]),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error::InvalidInput(format!("ADR {key} must contain strings")))
            })
            .collect(),
        _ => Err(Error::InvalidInput(format!("ADR {key} must be a list"))),
    }
}
fn header_fields(body: &str) -> Result<BTreeMap<String, String>> {
    let mut fields = BTreeMap::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            break;
        };
        let key = key.trim().to_lowercase();
        if !["status", "date", "deciders"].contains(&key.as_str()) {
            break;
        }
        if let Some(previous) = fields.insert(key, value.trim().to_owned()) {
            if previous != value.trim() {
                return Err(Error::SourceConflict(
                    "conflicting ADR header fields".into(),
                ));
            }
        }
    }
    Ok(fields)
}
fn without_header_fields(body: &str) -> String {
    let lines: Vec<_> = body.lines().collect();
    let start = lines
        .iter()
        .position(|line| {
            !line.trim().is_empty()
                && !line.split_once(':').is_some_and(|(key, _)| {
                    ["status", "date", "deciders"].contains(&key.trim().to_lowercase().as_str())
                })
        })
        .unwrap_or(lines.len());
    lines[start..].join("\n").trim().into()
}
