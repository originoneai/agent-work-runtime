#[allow(dead_code)]
#[path = "../../awr-store/tests/support/workstreams.rs"]
mod fixture;
use awr_core::*;
use awr_runtime::Runtime;
use awr_store::WorkstreamReadSelection;
use fixture::Fixture;
use sha2::{Digest, Sha256};

#[test]
fn scoped_file_read_checks_visibility_before_io_and_keeps_digest_guards() {
    let mut f = Fixture::new();
    let event = f.event(1, None);
    let body = b"Synthetic private report";
    let artifact = f
        .store
        .record_artifact(
            f.project,
            f.rev(),
            ArtifactDraft {
                artifact_type: "report".into(),
                locator: "private.txt".into(),
                sha256: format!("{:x}", Sha256::digest(body)),
                size: body.len() as u64,
                mime: "text/plain".into(),
                source_event_id: event.id,
            },
        )
        .unwrap()
        .0;
    let a = f.access(&[0]);
    let b = f.access(&[1]);
    let selector = WorkstreamReadSelection::default();
    // The path does not exist: denied scope must fail without observing that.
    let runtime = Runtime::attach(&mut f.store, f.project).unwrap();
    assert!(matches!(
        runtime.read_artifact_in_workstream(&a, &selector, artifact.id, 100),
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
    assert!(matches!(
        runtime.read_artifact_in_workstream(&b, &selector, artifact.id, 100),
        Err(Error::Io(_))
    ));
    std::fs::write(f.root.join("private.txt"), body).unwrap();
    let runtime = Runtime::attach(&mut f.store, f.project).unwrap();
    let (record, bytes) = runtime
        .read_artifact_in_workstream(&b, &selector, artifact.id, 100)
        .unwrap();
    assert_eq!(record.id, artifact.id);
    assert_eq!(bytes, body);
    assert!(matches!(
        runtime.read_artifact_in_workstream(&b, &selector, artifact.id, 2),
        Err(Error::InvalidInput(_))
    ));
    std::fs::write(f.root.join("private.txt"), vec![b'X'; body.len()]).unwrap();
    assert!(matches!(
        runtime.read_artifact_in_workstream(&b, &selector, artifact.id, 100),
        Err(Error::SourceConflict(_))
    ));
}

#[test]
fn scoped_evidence_read_uses_same_boundary_without_fabricating_verification() {
    let mut f = Fixture::new();
    let evidence = f.evidence(1);
    let a = f.access(&[0]);
    let b = f.access(&[1]);
    let selector = WorkstreamReadSelection::default();
    let runtime = Runtime::attach(&mut f.store, f.project).unwrap();
    assert!(matches!(
        runtime.read_evidence_report_in_workstream(&a, &selector, &evidence.external_key, 100),
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
    std::fs::write(f.root.join(&evidence.locator), b"Synthetic report").unwrap();
    let runtime = Runtime::attach(&mut f.store, f.project).unwrap();
    let (record, bytes) = runtime
        .read_evidence_report_in_workstream(&b, &selector, &evidence.external_key, 100)
        .unwrap();
    assert_eq!(bytes, b"Synthetic report");
    assert_eq!(record.item.level, EvidenceLevel::Designed);
    assert!(record.item.sha256.is_none());
}
