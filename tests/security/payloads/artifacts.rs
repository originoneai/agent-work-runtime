use awr_core::*;
use awr_runtime::{ArtifactFile, Runtime};
use awr_store::Store;
use std::{fs, path::PathBuf};

const IMPORT_CAP: u64 = 64 * 1024 * 1024;
const READ_CAP: u64 = 16 * 1024 * 1024;

struct Fixture {
    root: PathBuf,
    store: Store,
    project: Project,
    event: Event,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-artifact-boundary-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let mut store = Store::open(&root.join("state.db")).unwrap();
        let project = store
            .register_project(&root, "artifact-boundary", "Artifact boundary fixture")
            .unwrap();
        let event = store
            .append_event(
                project.id,
                project.project_revision,
                EventDraft::new("report.produced", "Produced a report"),
            )
            .unwrap();
        Self {
            root,
            store,
            project,
            event,
        }
    }
    fn revision(&self) -> u64 {
        self.store
            .project(self.project.id)
            .unwrap()
            .project_revision
    }
    fn import(&mut self, path: &str, max_bytes: u64) -> Result<(Artifact, Event)> {
        let revision = self.revision();
        Runtime::attach(&mut self.store, self.project.id)?.import_artifact(
            revision,
            ArtifactFile {
                path: self.root.join(path),
                artifact_type: "report".into(),
                mime: "application/octet-stream".into(),
                source_event_id: self.event.id,
                max_bytes,
            },
        )
    }
    fn read(&mut self, id: Id, cap: u64) -> Result<(Artifact, Vec<u8>)> {
        Runtime::attach(&mut self.store, self.project.id)?.read_artifact(id, cap)
    }
    fn files(&self) -> Vec<PathBuf> {
        let directory = self.root.join(".awr/artifacts");
        let mut entries = if directory.exists() {
            fs::read_dir(directory)
                .unwrap()
                .map(|r| r.unwrap().path())
                .collect()
        } else {
            Vec::new()
        };
        entries.sort();
        entries
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn managed_import_enforces_the_hard_cap_and_keeps_the_existing_artifact() {
    let mut f = Fixture::new();
    fs::write(f.root.join("small.bin"), b"small report").unwrap();
    let before = f.revision();
    for cap in [0, IMPORT_CAP + 1, u64::MAX] {
        assert!(
            matches!(f.import("small.bin", cap), Err(Error::InvalidInput(_))),
            "caller cannot raise the managed import cap"
        );
        assert_eq!(f.revision(), before);
        assert!(f.files().is_empty());
    }
    let path = f.root.join("report.bin");
    fs::File::create(&path)
        .unwrap()
        .set_len(IMPORT_CAP)
        .unwrap();
    let (artifact, _) = f.import("report.bin", IMPORT_CAP).unwrap();
    assert_eq!(artifact.size, IMPORT_CAP);
    assert_eq!(
        fs::metadata(f.root.join(&artifact.locator)).unwrap().len(),
        IMPORT_CAP
    );
    let existing = f.files();
    let revision = f.revision();
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(IMPORT_CAP + 1)
        .unwrap();
    assert!(matches!(
        f.import("report.bin", IMPORT_CAP),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(f.revision(), revision);
    assert_eq!(f.files(), existing);
    assert_eq!(
        f.store.artifact(f.project.id, artifact.id).unwrap().sha256,
        artifact.sha256
    );
    assert!(
        f.store
            .events_since(f.project.id, revision, 100)
            .unwrap()
            .is_empty()
    );
    println!("AWR_PAYLOAD_CASE artifact_import_limit");
}

#[test]
fn artifact_reads_check_exact_limit_growth_and_digest_without_changing_state() {
    let mut f = Fixture::new();
    fs::File::create(f.root.join("report.bin"))
        .unwrap()
        .set_len(READ_CAP)
        .unwrap();
    let (artifact, _) = f.import("report.bin", IMPORT_CAP).unwrap();
    let revision = f.revision();
    let (_, bytes) = f.read(artifact.id, READ_CAP).unwrap();
    assert_eq!(bytes.len() as u64, READ_CAP);
    assert!(bytes.iter().all(|b| *b == 0));
    for cap in [0, READ_CAP - 1, READ_CAP + 1, u64::MAX] {
        assert!(matches!(
            f.read(artifact.id, cap),
            Err(Error::InvalidInput(_))
        ));
    }
    let managed = f.root.join(&artifact.locator);
    let mut file = fs::OpenOptions::new().write(true).open(&managed).unwrap();
    file.set_len(READ_CAP + 1).unwrap();
    assert!(matches!(
        f.read(artifact.id, READ_CAP),
        Err(Error::InvalidInput(_))
    ));
    file.set_len(READ_CAP).unwrap();
    use std::io::Write;
    file.write_all(b"changed report").unwrap();
    file.sync_all().unwrap();
    assert!(matches!(
        f.read(artifact.id, READ_CAP),
        Err(Error::SourceConflict(_))
    ));
    assert_eq!(f.revision(), revision);
    assert!(
        f.store
            .events_since(f.project.id, revision, 100)
            .unwrap()
            .is_empty()
    );
    println!("AWR_PAYLOAD_CASE artifact_read_limit");
}

#[test]
fn registered_reports_apply_the_read_cap_without_a_recorded_size() {
    let mut f = Fixture::new();
    let path = f.root.join("report.bin");
    fs::File::create(&path)
        .unwrap()
        .set_len(READ_CAP + 1)
        .unwrap();
    let revision = f.revision();
    f.store
        .record_evidence(
            f.project.id,
            revision,
            EvidenceDraft {
                external_key: "report".into(),
                work_item_key: None,
                evidence_type: "report".into(),
                level: EvidenceLevel::Implemented,
                summary: "Explicit report reference".into(),
                locator: "report.bin".into(),
                sha256: None,
                source_sha: None,
                command: None,
                scope: vec!["*".into()],
                branch_id: None,
                verified_at: None,
            },
        )
        .unwrap();
    let revision = f.revision();
    let runtime = Runtime::attach(&mut f.store, f.project.id).unwrap();
    assert!(matches!(
        runtime.read_evidence_report("report", READ_CAP),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        runtime.read_evidence_report("report", u64::MAX),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(f.revision(), revision);
}

#[cfg(unix)]
#[test]
fn managed_storage_cannot_be_a_directory_alias_even_inside_the_project() {
    use std::os::unix::fs::symlink;
    let mut f = Fixture::new();
    fs::write(f.root.join("report.txt"), b"Original report").unwrap();
    fs::create_dir_all(f.root.join(".awr")).unwrap();
    fs::create_dir(f.root.join(".awr/other-storage")).unwrap();
    symlink(
        f.root.join(".awr/other-storage"),
        f.root.join(".awr/artifacts"),
    )
    .unwrap();
    let revision = f.revision();
    assert!(matches!(
        f.import("report.txt", 1024),
        Err(Error::RuleViolation(_))
    ));
    assert_eq!(f.revision(), revision);
    assert_eq!(
        fs::read_dir(f.root.join(".awr/other-storage"))
            .unwrap()
            .count(),
        0
    );
}
