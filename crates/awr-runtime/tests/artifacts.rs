#[allow(dead_code)]
#[path = "../../awr-store/tests/support/mod.rs"]
mod support;
use awr_core::*;
use awr_runtime::{ArtifactFile, Runtime};
use sha2::{Digest, Sha256};
use support::Fixture;

#[test]
fn artifact_copy_keeps_bytes_external_and_verifiable() {
    let mut f = Fixture::new();
    let body = vec![0x5au8; 1024 * 1024];
    let path = f.root.join("large-report.bin");
    std::fs::write(&path, &body).unwrap();
    let revision = f.store.project(f.project.id).unwrap().project_revision;
    let event = f
        .store
        .append_event(
            f.project.id,
            revision,
            EventDraft::new("report.produced", "Produced a large report"),
        )
        .unwrap();
    let (artifact, recorded) = Runtime::attach(&mut f.store, f.project.id)
        .unwrap()
        .import_artifact(
            event.project_revision,
            ArtifactFile {
                path: path.clone(),
                artifact_type: "report".into(),
                mime: "application/octet-stream".into(),
                source_event_id: event.id,
                max_bytes: 2 * 1024 * 1024,
            },
        )
        .unwrap();
    let managed = f.root.join(&artifact.locator);
    assert_eq!(std::fs::read(&managed).unwrap(), body);
    assert_eq!(artifact.sha256, format!("{:x}", Sha256::digest(&body)));
    assert_eq!(artifact.size, body.len() as u64);
    std::fs::write(&path, b"changed original").unwrap();
    assert_eq!(std::fs::read(&managed).unwrap(), body);
    assert_eq!(
        f.store
            .artifact(f.project.id, artifact.id)
            .unwrap()
            .source_event_id,
        Some(event.id)
    );
    assert_eq!(recorded.payload["size"].as_u64(), Some(body.len() as u64));
    assert!(f.store.doctor().unwrap().ok);
}

#[test]
fn artifact_limits_and_failed_registration_leave_no_managed_file() {
    let mut f = Fixture::new();
    let path = f.root.join("report.txt");
    std::fs::write(&path, b"report body").unwrap();
    let revision = f.store.project(f.project.id).unwrap().project_revision;
    let event = f
        .store
        .append_event(
            f.project.id,
            revision,
            EventDraft::new("report.produced", "Produced report"),
        )
        .unwrap();
    let request = |limit, mime: &str| ArtifactFile {
        path: path.clone(),
        artifact_type: "report".into(),
        mime: mime.into(),
        source_event_id: event.id,
        max_bytes: limit,
    };
    assert!(matches!(
        Runtime::attach(&mut f.store, f.project.id)
            .unwrap()
            .import_artifact(event.project_revision, request(2, "text/plain")),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        Runtime::attach(&mut f.store, f.project.id)
            .unwrap()
            .import_artifact(event.project_revision, request(100, "")),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        std::fs::read_dir(f.root.join(".awr/artifacts"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        event.project_revision
    );
    assert!(matches!(
        Runtime::attach(&mut f.store, f.project.id)
            .unwrap()
            .import_artifact(revision, request(100, "text/plain")),
        Err(Error::RevisionConflict { .. })
    ));
}
