use super::*;

struct Fixture {
    base: std::path::PathBuf,
    root: std::path::PathBuf,
    outside: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("awr-write-handles-{}", Id::new()));
        fs::create_dir_all(base.join("project/docs")).unwrap();
        fs::create_dir_all(base.join("project/.awr")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        let base = base.canonicalize().unwrap();
        let f = Self {
            root: base.join("project"),
            outside: base.join("outside"),
            base,
        };
        fs::write(f.root.join("docs/work.yaml"), b"original source").unwrap();
        fs::write(
            f.outside.join("work.yaml"),
            b"outside source must stay unchanged",
        )
        .unwrap();
        f
    }
    fn replacement(&self) -> SourceReplacement {
        let path = self.root.join("docs/work.yaml");
        SourceReplacement::prepare(
            &path,
            b"reviewed replacement",
            fs::metadata(&path).unwrap().permissions(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn replacement_uses_the_held_parent_after_path_redirection() {
    let f = Fixture::new();
    let mut replacement = f.replacement();
    let name = replacement.temp.clone().unwrap();
    fs::write(f.outside.join(&name), b"outside temp trap").unwrap();
    fs::rename(f.root.join("docs"), f.root.join("held-docs")).unwrap();
    std::os::unix::fs::symlink(&f.outside, f.root.join("docs")).unwrap();
    replacement.install().unwrap();
    assert_eq!(
        fs::read(f.outside.join("work.yaml")).unwrap(),
        b"outside source must stay unchanged"
    );
    assert_eq!(
        fs::read(f.outside.join(&name)).unwrap(),
        b"outside temp trap"
    );
    assert_eq!(
        fs::read(f.root.join("held-docs/work.yaml")).unwrap(),
        b"reviewed replacement"
    );
    assert!(replacement.install().is_err());
    // Dropping an installed replacement must not remove a later file using its old name.
    fs::write(f.root.join("held-docs").join(&name), b"later file").unwrap();
    drop(replacement);
    assert_eq!(
        fs::read(f.root.join("held-docs").join(&name)).unwrap(),
        b"later file"
    );
    println!("AWR_PATH_CASE write_parent_replacement");
}

#[cfg(unix)]
#[test]
fn abandoned_temp_cleanup_does_not_follow_a_changed_parent() {
    let f = Fixture::new();
    let replacement = f.replacement();
    let name = replacement.temp.clone().unwrap();
    fs::write(f.outside.join(&name), b"outside temp trap").unwrap();
    fs::rename(f.root.join("docs"), f.root.join("held-docs")).unwrap();
    std::os::unix::fs::symlink(&f.outside, f.root.join("docs")).unwrap();
    drop(replacement);
    assert!(!f.root.join("held-docs").join(&name).exists());
    assert_eq!(
        fs::read(f.outside.join(&name)).unwrap(),
        b"outside temp trap"
    );
    assert_eq!(
        fs::read(f.root.join("held-docs/work.yaml")).unwrap(),
        b"original source"
    );
}

#[cfg(unix)]
#[test]
fn recovery_and_lock_aliases_cannot_create_or_change_outside_files() {
    let f = Fixture::new();
    let source = Id::new();
    std::os::unix::fs::symlink(&f.outside, f.root.join(".awr/mutations")).unwrap();
    assert!(matches!(
        source_lock(&f.root, source),
        Err(Error::RuleViolation(_))
    ));
    assert!(!f.outside.join(format!("{source}.lock")).exists());
    fs::remove_file(f.root.join(".awr/mutations")).unwrap();
    fs::create_dir(f.root.join(".awr/mutations")).unwrap();
    std::os::unix::fs::symlink(
        f.outside.join("work.yaml"),
        f.root.join(format!(".awr/mutations/{source}.lock")),
    )
    .unwrap();
    assert!(matches!(
        source_lock(&f.root, source),
        Err(Error::RuleViolation(_))
    ));
    assert_eq!(
        fs::read(f.outside.join("work.yaml")).unwrap(),
        b"outside source must stay unchanged"
    );
}

#[test]
fn exclusive_creation_keeps_existing_files_and_snapshots_round_trip() {
    let f = Fixture::new();
    let directory = open_dir_exact(&f.root.join("docs")).unwrap();
    assert!(
        create_file(
            &directory,
            OsStr::new("work.yaml"),
            b"must not overwrite",
            None
        )
        .is_err()
    );
    assert_eq!(
        fs::read(f.root.join("docs/work.yaml")).unwrap(),
        b"original source"
    );
    let before = b"before";
    let after = b"after";
    let plan = MutationWritePlan {
        id: Id::new(),
        before_fingerprint: fingerprint(before),
        after_fingerprint: fingerprint(after),
        before_size: before.len() as u64,
        after_size: after.len() as u64,
        target_after_hash: "a".repeat(64),
    };
    stage(&f.root, &plan, before, after).unwrap();
    assert_eq!(stored_after(&f.root, &plan).unwrap(), after);
    assert!(stage(&f.root, &plan, b"changed", b"changed").is_err());
    assert_eq!(stored_after(&f.root, &plan).unwrap(), after);
}
