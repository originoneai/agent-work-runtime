use awr_core::{AuthorityMode, Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub project: ProjectConfig,
    pub sources: Vec<SourceSpec>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    pub external_key: Option<String>,
    #[serde(default = "source_first")]
    pub authority_mode: AuthorityMode,
    #[serde(default)]
    pub authorized_roots: Vec<PathBuf>,
}
fn source_first() -> AuthorityMode {
    AuthorityMode::SourceFirst
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub domain: String,
    pub role: String,
    pub path: Option<PathBuf>,
    pub locator: Option<String>,
    pub adapter: String,
    #[serde(default)]
    pub options: toml::Table,
}
impl Manifest {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let path = root
            .join(".awr/project.toml")
            .canonicalize()
            .map_err(|e| Error::SourceUnavailable(e.to_string()))?;
        if !path.starts_with(&root) {
            return Err(Error::RuleViolation("manifest escapes project root".into()));
        }
        let bytes = crate::read_source_capped(&path, 64 * 1024)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| Error::InvalidInput(e.to_string()))?;
        Self::parse(text)
    }
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(text)
            .map_err(|e| Error::InvalidInput(format!("source manifest: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<()> {
        if self.project.name.trim().is_empty() || self.sources.is_empty() {
            return Err(Error::InvalidInput(
                "manifest needs a project name and at least one source".into(),
            ));
        }
        if self
            .project
            .external_key
            .as_ref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err(Error::InvalidInput(
                "project external_key must not be empty".into(),
            ));
        }
        let mut unique = BTreeSet::new();
        for source in &self.sources {
            if !["goal", "plan", "rules", "ledger", "decisions"].contains(&source.domain.as_str())
                || !["primary", "supporting"].contains(&source.role.as_str())
            {
                return Err(Error::InvalidInput(format!(
                    "invalid source domain/role: {}/{}",
                    source.domain, source.role
                )));
            }
            if ![
                "yaml-ledger-v1",
                "markdown-heading-v1",
                "markdown-rules-v1",
                "markdown-directory-v1",
            ]
            .contains(&source.adapter.as_str())
            {
                return Err(Error::Unsupported(format!(
                    "source adapter {}",
                    source.adapter
                )));
            }
            if source.path.is_some() == source.locator.is_some() {
                return Err(Error::InvalidInput(
                    "each source needs exactly one path or locator".into(),
                ));
            }
            if source
                .path
                .as_ref()
                .is_some_and(|p| p.as_os_str().is_empty())
                || source.locator.as_ref().is_some_and(|p| p.is_empty())
            {
                return Err(Error::InvalidInput("empty source path/locator".into()));
            }
            let location = source
                .locator
                .clone()
                .unwrap_or_else(|| source.path.as_ref().unwrap().to_string_lossy().into_owned());
            if !unique.insert((source.domain.clone(), location)) {
                return Err(Error::InvalidInput(
                    "duplicate source domain/location".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn authorized_roots(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let root = root.canonicalize()?;
        let mut roots = vec![root.clone()];
        for extra in &self.project.authorized_roots {
            let path = if extra.is_absolute() {
                extra.clone()
            } else {
                root.join(extra)
            }
            .canonicalize()?;
            if !path.is_dir() {
                return Err(Error::InvalidInput(
                    "authorized root must be a directory".into(),
                ));
            }
            roots.push(path);
        }
        roots.sort();
        roots.dedup();
        Ok(roots)
    }
}
