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
    source_warnings: usize,
    source_state_fingerprint: String,
}
impl ReadProject {
    pub fn open(root: &Path) -> Result<Self> {
        let store = Store::read_snapshot(&database(root)?, 256 * 1024 * 1024)?;
        let project = store.project_by_root(root)?;
        let source_state_fingerprint = awr_source::source_state_fingerprint(&store, project.id)?;
        let mut view = Self {
            source_state_fingerprint,
            store,
            project,
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
        self.source_warnings = refresh
            .sources
            .iter()
            .map(|source| source.warnings.len())
            .sum();
        Ok(())
    }
    pub fn metadata(&self) -> Value {
        json!({"ok":true,"project_revision":self.project.project_revision,"freshness_basis":"source_verified_readonly","source_refresh_performed":false,"read_only":true,"source_issues":[],"source_warnings":self.source_warnings,
            "snapshot":{"version":1,"storage":"private_memory","coherent":true,"project_revision":self.project.project_revision,
            "source_state_fingerprint":self.source_state_fingerprint,"source_refresh_revision":null,"source_currentness_verified":true}})
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_events_do_not_invalidate_a_read_snapshot_but_source_changes_do() {
        let root = std::env::temp_dir().join(format!("awr-mcp-snapshot-{}", Id::new()));
        std::fs::create_dir_all(root.join(".awr")).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join(".awr/project.toml"),"[project]\nname='Snapshot fixture'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        std::fs::write(
            root.join("work.yaml"),
            "work_items:\n- id: W\n  title: Write a guide\n  status: ready\n",
        )
        .unwrap();
        let mut origin = Store::open(&root.join(".awr/state.db")).unwrap();
        let report =
            index_project(&mut origin, &root, &Manifest::load(&root).unwrap(), false).unwrap();
        let mut read = ReadProject::open(&root).unwrap();
        origin
            .append_event(
                report.project_id,
                report.project_revision,
                EventDraft::new("work.observed", "Another reader observed the outline"),
            )
            .unwrap();
        read.finish(&root).unwrap();
        assert_eq!(read.project.project_revision, report.project_revision);
        assert!(
            origin.project(report.project_id).unwrap().project_revision
                > read.project.project_revision
        );
        assert_eq!(read.metadata()["snapshot"]["coherent"], true);
        std::fs::remove_file(root.join("work.yaml")).unwrap();
        assert!(read.finish(&root).is_err());
        drop(read);
        drop(origin);
        std::fs::remove_dir_all(root).unwrap();
    }
}
