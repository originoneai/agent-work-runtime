use awr_core::{DecisionStatus, Error, Freshness, Id, Source};
use awr_source::{
    Manifest, MarkdownDirectoryAdapter, ParseContext, SourceAdapter, SourceSnapshot, fingerprint,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("awr-directory-{}", Id::new()));
        fs::create_dir_all(path.join("docs")).unwrap();
        fs::write(
            path.join("docs/001.md"),
            include_str!("../../../tests/fixtures/decisions/001-persistence.md"),
        )
        .unwrap();
        fs::write(
            path.join("docs/002.md"),
            include_str!("../../../tests/fixtures/decisions/002-obsolete.md"),
        )
        .unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn manifest() -> Manifest {
    Manifest::parse("[project]\nname='Directory fixture'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='docs'\nadapter='markdown-directory-v1'\n").unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn parse(text: &str) -> awr_core::Result<awr_core::ProjectionBatch> {
    let source = Source {
        id: Id::new(),
        project_id: Id::new(),
        domain: "decisions".into(),
        role: "supporting".into(),
        locator: "file:///fixture/adr.md".into(),
        format: "markdown".into(),
        adapter: "markdown-directory-v1".into(),
        revision: 0,
        fingerprint: String::new(),
        freshness: Freshness::Stale,
    };
    let snapshot = SourceSnapshot {
        locator: source.locator.clone(),
        fingerprint: fingerprint(text.as_bytes()),
        bytes: text.as_bytes().into(),
    };
    MarkdownDirectoryAdapter.parse(
        &snapshot,
        &ParseContext {
            source: &source,
            existing_ids: BTreeMap::new(),
        },
        &manifest().sources[0],
    )
}

#[test]
fn adr_status_provenance_and_selected_body() {
    let batch = parse(include_str!(
        "../../../tests/fixtures/decisions/001-persistence.md"
    ))
    .unwrap();
    let decision = &batch.decisions[0];
    assert_eq!(decision.status, DecisionStatus::Accepted);
    assert_eq!(decision.raw_status, "Accepted");
    assert_eq!(decision.title, "Persist runtime state");
    assert!(decision.decision.contains("Commit derived facts"));
    assert!(!decision.decision.contains("UNRELATED_RAW_TRANSCRIPT"));
    assert!(!decision.rationale.contains("UNRELATED_RAW_TRANSCRIPT"));
    assert_eq!(decision.affected_keys, ["W1"]);
    assert_eq!(decision.paths, ["src/runtime"]);
    assert_eq!(decision.meta.source_ref.start_line, Some(1));
    assert!(batch.warnings.is_empty());
    assert_eq!(
        parse(include_str!(
            "../../../tests/fixtures/decisions/002-obsolete.md"
        ))
        .unwrap()
        .decisions[0]
            .status,
        DecisionStatus::Superseded
    );
    let unknown =
        parse("# Experimental\n\nStatus: being_discussed\n\nPending a decision.\n").unwrap();
    assert_eq!(unknown.decisions[0].raw_status, "being_discussed");
    assert_eq!(unknown.decisions[0].status, DecisionStatus::Unknown);
    assert!(!unknown.warnings.is_empty());
    assert!(matches!(
        parse("# Conflict\n\nStatus: accepted\nStatus: superseded\n"),
        Err(Error::SourceConflict(_))
    ));
}

#[test]
fn directory_inventory_reports_added_modified_removed() {
    let fixture = Fixture::new();
    let manifest = manifest();
    let spec = &manifest.sources[0];
    let adapter = MarkdownDirectoryAdapter;
    fs::write(fixture.0.join("docs/ignore.txt"), "unrelated").unwrap();
    let before = adapter.scan(&fixture.0, &manifest, spec, 65536).unwrap();
    assert_eq!(before.files.len(), 2);
    fs::write(
        fixture.0.join("docs/001.md"),
        "# Updated\n\nStatus: accepted\n\nUpdated decision.",
    )
    .unwrap();
    fs::remove_file(fixture.0.join("docs/002.md")).unwrap();
    fs::create_dir(fixture.0.join("docs/nested")).unwrap();
    fs::write(
        fixture.0.join("docs/nested/003.MD"),
        "# New\n\nStatus: proposed\n\nNew proposal.",
    )
    .unwrap();
    let after = adapter.scan(&fixture.0, &manifest, spec, 65536).unwrap();
    let delta = after.diff(&before);
    assert_eq!(
        (delta.added.len(), delta.modified.len(), delta.removed.len()),
        (1, 1, 1)
    );
    assert!(delta.added[0].ends_with("/nested/003.MD"));
    assert!(delta.modified[0].ends_with("/001.md"));
    assert!(delta.removed[0].ends_with("/002.md"));
    assert!(after.diff(&after).modified.is_empty());
}

#[test]
fn git_directory_pins_reads_and_preserves_ref_identity() {
    let fixture = Fixture::new();
    git(&fixture.0, &["init", "--initial-branch=main"]);
    git(&fixture.0, &["config", "user.name", "AWR Fixture"]);
    git(
        &fixture.0,
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(&fixture.0, &["config", "commit.gpgsign", "false"]);
    git(&fixture.0, &["add", "docs"]);
    git(&fixture.0, &["commit", "-m", "Initial fixture"]);
    let mut manifest = manifest();
    manifest.sources[0].path = None;
    manifest.sources[0].locator = Some("git://HEAD:docs".into());
    let spec = &manifest.sources[0];
    let adapter = MarkdownDirectoryAdapter;
    let before = adapter.scan(&fixture.0, &manifest, spec, 65536).unwrap();
    assert!(before.files.contains_key("git://HEAD:docs/001.md"));
    let pinned = adapter.discover(&fixture.0, &manifest, spec).unwrap();
    let old = pinned[0].read(&fixture.0, 65536).unwrap();
    fs::write(
        fixture.0.join("docs/001.md"),
        "# Changed\n\nStatus: accepted\n\nChanged Git decision.",
    )
    .unwrap();
    git(&fixture.0, &["add", "docs/001.md"]);
    git(&fixture.0, &["commit", "-m", "Changed fixture"]);
    assert_eq!(
        pinned[0].read(&fixture.0, 65536).unwrap().fingerprint,
        old.fingerprint
    );
    let after = adapter.scan(&fixture.0, &manifest, spec, 65536).unwrap();
    let delta = after.diff(&before);
    assert!(delta.added.is_empty() && delta.removed.is_empty());
    assert_eq!(delta.modified.len(), 2); // Git fingerprints bind the immutable commit as well as blob bytes.
    manifest.sources[0].locator = Some("git://HEAD:.".into());
    assert_eq!(
        adapter
            .discover(&fixture.0, &manifest, &manifest.sources[0])
            .unwrap()
            .len(),
        2
    );
}
