use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(crate) fn database(root: &Path) -> Result<PathBuf> {
    let runtime = root.join(".awr");
    let database = runtime.join("state.db");
    for path in [&runtime, &database] {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound("AWR database; initialize this project first".into())
            } else {
                Error::Io(error)
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(Error::RuleViolation(
                "MCP runtime directory/database must not be a symbolic link".into(),
            ));
        }
    }
    if !runtime.is_dir()
        || !database.is_file()
        || database.canonicalize()?.parent() != Some(runtime.as_path())
    {
        return Err(Error::InvalidInput(
            "MCP requires an existing project-local AWR database".into(),
        ));
    }
    Ok(database)
}

pub(crate) struct ReadProject {
    pub store: Store,
    pub project: Project,
    origin: Store,
    source_warnings: usize,
}
impl ReadProject {
    pub fn open(root: &Path) -> Result<Self> {
        let origin = Store::open_readonly(&database(root)?)?;
        let project = origin.project_by_root(root)?;
        let store = origin.memory_snapshot(256 * 1024 * 1024)?;
        let actual = store.project(project.id)?.project_revision;
        if actual != project.project_revision {
            return Err(Error::RevisionConflict {
                expected: project.project_revision,
                actual,
            });
        }
        let mut view = Self {
            store,
            project,
            origin,
            source_warnings: 0,
        };
        view.finish(root)?;
        Ok(view)
    }
    pub fn finish(&mut self, root: &Path) -> Result<()> {
        // The exact CLI indexer runs against disposable RAM. Reject any changed projection
        // or registration; never publish an ephemeral revision as real project state.
        let revision = self.store.project(self.project.id)?.project_revision;
        if revision != self.project.project_revision {
            return Err(Error::SourceStale(
                "the read snapshot changed; run awr source reindex and inspect again".into(),
            ));
        }
        let refresh = index_project(&mut self.store, root, &Manifest::load(root)?, false)?;
        if !refresh.ok
            || refresh.pending != 0
            || refresh.indexed != 0
            || refresh.retired != 0
            || refresh.project_revision != revision
        {
            return Err(Error::SourceStale("source files or mappings differ from their indexed projection; run awr source reindex and inspect again".into()));
        }
        let actual = self.origin.project(self.project.id)?.project_revision;
        if actual != revision {
            return Err(Error::RevisionConflict {
                expected: revision,
                actual,
            });
        }
        self.source_warnings = refresh
            .sources
            .iter()
            .map(|source| source.warnings.len())
            .sum();
        Ok(())
    }
    pub fn metadata(&self) -> Value {
        json!({"ok":true,"project_revision":self.project.project_revision,"freshness_basis":"source_verified_readonly","source_refresh_performed":false,"read_only":true,"source_issues":[],"source_warnings":self.source_warnings})
    }
}

pub(crate) fn write_project(root: &Path, expected: Revision) -> Result<(Store, Project)> {
    let mut store = Store::open_existing(&database(root)?)?;
    let project = store.project_by_root(root)?;
    if project.project_revision != expected {
        return Err(Error::RevisionConflict {
            expected,
            actual: project.project_revision,
        });
    }
    let refresh = index_project(&mut store, root, &Manifest::load(root)?, false)?;
    if !refresh.ok || refresh.pending != 0 {
        return Err(Error::SourceStale(
            "source refresh is incomplete; inspect awr source reindex".into(),
        ));
    }
    let project = store.project(project.id)?;
    if project.project_revision != expected {
        return Err(Error::RevisionConflict {
            expected,
            actual: project.project_revision,
        });
    }
    Ok((store, project))
}
