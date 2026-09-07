//! Focused source lifecycle check, kept separate from the later system acceptance suite.
use awr_core::{EntityKind, Error, Freshness, Goal, ProjectionBatch, Result, Source};
use awr_source::{Locator, ParseContext, SourceSnapshot, observe_source};
use awr_store::{SourceRegistration, Store};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn batch(store: &Store, source: &Source, snapshot: &SourceSnapshot) -> Result<ProjectionBatch> {
    let context = ParseContext {
        source,
        existing_ids: store.projection_ids(source)?,
    };
    Ok(ProjectionBatch {
        goals: vec![Goal {
            meta: context.meta(
                EntityKind::Goal,
                "G1",
                snapshot,
                Some("#goal".into()),
                Some((1, 2)),
            )?,
            title: snapshot.section_text(1, 2)?.into(),
            status: "active".into(),
            priority: None,
            success_criteria: vec!["Source and projection agree".into()],
            summary: String::new(),
        }],
        ..Default::default()
    })
}

fn main() -> Result<()> {
    let root = PathBuf::from(
        env::args()
            .nth(1)
            .ok_or_else(|| Error::InvalidInput("provide an empty fixture directory".into()))?,
    )
    .canonicalize()?;
    let path = root.join("goals.md");
    if root.join("state.db").exists() {
        return Err(Error::InvalidInput(
            "fixture database already exists".into(),
        ));
    }
    fs::write(&path, "# Goal\nOne.\n# Other\nAlpha.\n")?;
    let locator = Locator::File(path.clone());
    let first = locator.read(&root, 4096)?;
    let mut store = Store::open(&root.join("state.db"))?;
    let project = store.register_project(&root, "source-lifecycle", "Source lifecycle")?;
    let initial = store.register_source(
        project.id,
        &SourceRegistration {
            domain: "goal",
            role: "primary",
            locator: &first.locator,
            format: "markdown",
            adapter: "lifecycle-fixture",
        },
    )?;
    let data = batch(&store, &initial, &first)?;
    let first_source = store.commit_source_projection(&initial, &first.fingerprint, data)?;
    assert_eq!(first_source.revision, 1);
    assert_eq!(first_source.freshness, Freshness::Fresh);
    let id = store.projection_ids(&first_source)?[&(EntityKind::Goal, "G1".into())];
    let before = store.project(project.id)?.project_revision;
    let unchanged = observe_source(&mut store, &first_source, &root, &locator, 4096)?;
    assert!(!unchanged.changed);
    store.commit_source_projection(
        &first_source,
        &first.fingerprint,
        batch(&store, &first_source, &first)?,
    )?;
    assert_eq!(store.project(project.id)?.project_revision, before);

    fs::write(&path, "# Goal\nOne.\n# Other\nBeta.\n")?;
    let observed = observe_source(&mut store, &first_source, &root, &locator, 4096)?;
    assert!(observed.changed);
    assert_eq!(observed.source.freshness, Freshness::Stale);
    assert_eq!(observed.source.fingerprint, first.fingerprint);
    assert_eq!(observed.source.revision, 1);
    let second = observed.snapshot.unwrap();
    assert_eq!(
        first.section_fingerprint(1, 2)?,
        second.section_fingerprint(1, 2)?
    );
    assert_ne!(
        first.section_fingerprint(3, 4)?,
        second.section_fingerprint(3, 4)?
    );
    let data = batch(&store, &observed.source, &second)?;
    let second_source =
        store.commit_source_projection(&observed.source, &second.fingerprint, data)?;
    assert_eq!(second_source.revision, 2);
    assert_eq!(
        store.projection_ids(&second_source)?[&(EntityKind::Goal, "G1".into())],
        id
    );

    fs::write(&path, "# Goal\nTwo.\n# Other\nBeta.\n")?;
    let third = locator.read(&root, 4096)?;
    let mut invalid = batch(&store, &second_source, &third)?;
    invalid.goals[0].meta.source_ref.source_fingerprint = "incorrect".into();
    let failed = store
        .commit_source_projection(&second_source, &third.fingerprint, invalid)
        .unwrap_err();
    assert!(matches!(failed, Error::SourceConflict(_)));
    let stale = store.source(project.id, initial.id)?;
    assert_eq!(stale.freshness, Freshness::Stale);
    assert_eq!(stale.revision, 2);
    assert_eq!(stale.fingerprint, second.fingerprint);
    assert_eq!(
        store.source_projection_payloads(&stale, EntityKind::Goal)?[0]["title"],
        "# Goal\nOne.\n"
    );
    let data = batch(&store, &stale, &third)?;
    let third_source = store.commit_source_projection(&stale, &third.fingerprint, data)?;
    assert_eq!(third_source.revision, 3);
    assert!(matches!(
        store.commit_source_projection(
            &first_source,
            &first.fingerprint,
            ProjectionBatch::default()
        ),
        Err(Error::SourceConflict(_))
    ));
    assert!(matches!(
        store.mark_source_freshness(&third_source, Freshness::Fresh),
        Err(Error::InvalidInput(_))
    ));

    fs::remove_file(&path)?;
    let missing = observe_source(&mut store, &third_source, &root, &locator, 4096)?;
    assert_eq!(missing.source.freshness, Freshness::Unavailable);
    assert!(matches!(missing.error, Some(Error::SourceUnavailable(_))));
    assert_eq!(missing.source.revision, 3);
    assert_eq!(missing.source.fingerprint, third.fingerprint);
    assert_eq!(
        store.source_projection_payloads(&missing.source, EntityKind::Goal)?[0]["title"],
        "# Goal\nTwo.\n"
    );
    fs::write(&path, &third.bytes)?;
    let returned = observe_source(&mut store, &missing.source, &root, &locator, 4096)?;
    assert!(!returned.changed);
    assert_eq!(returned.source.freshness, Freshness::Stale);
    let data = batch(&store, &returned.source, &third)?;
    let recovered = store.commit_source_projection(&returned.source, &third.fingerprint, data)?;
    assert_eq!(recovered.freshness, Freshness::Fresh);
    assert_eq!(recovered.revision, 4);
    let events = store.events_since(project.id, 0, 100)?;
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "source.projected")
            .count(),
        4
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"ok":true,"successful_projections":4,
        "source_revision":recovered.revision,"entity_identity_preserved":true,"unchanged_index_is_noop":true,
        "section_change_isolated":true,"failed_projection_retains_stale_data":true,
        "unavailable_retains_last_fingerprint":true,"recovery_requires_projection":true,
        "stale_writer_rejected":true,"events":events.len(),"doctor":Store::inspect(&root.join("state.db"))?}))?
    );
    Ok(())
}
