use awr_core::Id;
use awr_source::{Locator, Manifest, MarkdownDirectoryAdapter, SourceAdapter};
use std::{fs, path::PathBuf};

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("awr-security-paths-{}", Id::new()));
        let root = base.join("project");
        let outside = base.join("outside");
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("docs/source.md"), "AUTHORIZED_SOURCE").unwrap();
        fs::write(outside.join("source.md"), "OUTSIDE_AUTHORITY_SENTINEL").unwrap();
        Self {
            base,
            root,
            outside,
        }
    }
    fn manifest(&self, directory: bool) -> Manifest {
        Manifest::parse(&format!("[project]\nname='Path boundary fixture'\n[[sources]]\ndomain='{}'\nrole='primary'\npath='{}'\nadapter='{}'\n",
            if directory { "decisions" } else { "goal" },
            if directory { "docs" } else { "docs/source.md" },
            if directory { "markdown-directory-v1" } else { "markdown-heading-v1" })).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn resolved_file_cannot_follow_a_replaced_leaf() {
    let f = Fixture::new();
    let manifest = f.manifest(false);
    let locator = Locator::from_spec(&f.root, &manifest, &manifest.sources[0]).unwrap();
    fs::remove_file(f.root.join("docs/source.md")).unwrap();
    std::os::unix::fs::symlink(f.outside.join("source.md"), f.root.join("docs/source.md")).unwrap();
    let observed = locator.read(&f.root, 1024);
    assert!(
        observed.is_err(),
        "resolved source followed a replaced leaf: {observed:?}"
    );
    println!("AWR_PATH_CASE replaced_leaf");
}

#[cfg(unix)]
#[test]
fn resolved_file_cannot_follow_a_replaced_parent() {
    let f = Fixture::new();
    let manifest = f.manifest(false);
    let locator = Locator::from_spec(&f.root, &manifest, &manifest.sources[0]).unwrap();
    fs::rename(f.root.join("docs"), f.root.join("original-docs")).unwrap();
    std::os::unix::fs::symlink(&f.outside, f.root.join("docs")).unwrap();
    let observed = locator.read(&f.root, 1024);
    assert!(
        observed.is_err(),
        "resolved source followed a replaced parent: {observed:?}"
    );
    println!("AWR_PATH_CASE replaced_parent");
}

#[cfg(unix)]
#[test]
fn discovered_child_cannot_follow_a_replaced_leaf() {
    let f = Fixture::new();
    let manifest = f.manifest(true);
    let locators = MarkdownDirectoryAdapter
        .discover(&f.root, &manifest, &manifest.sources[0])
        .unwrap();
    assert_eq!(locators.len(), 1);
    fs::remove_file(f.root.join("docs/source.md")).unwrap();
    std::os::unix::fs::symlink(f.outside.join("source.md"), f.root.join("docs/source.md")).unwrap();
    let observed = locators[0].read(&f.root, 1024);
    assert!(
        observed.is_err(),
        "discovered child escaped during read: {observed:?}"
    );
    println!("AWR_PATH_CASE discovered_replaced_leaf");
}

#[test]
fn source_mapping_matrix_preserves_explicit_authority() {
    let contract: serde_json::Value = serde_json::from_str(include_str!("contract.json")).unwrap();
    assert_eq!(
        contract["target_cases"],
        contract["cases"].as_array().unwrap().len()
    );
    for id in [
        "relative_inside",
        "absolute_inside",
        "absolute_outside",
        "prefix_sibling",
        "parent_traversal",
        "file_uri_inside",
        "file_uri_outside",
        "authorized_absolute_root",
        "authorized_relative_root",
        "authorized_file_uri",
    ] {
        let f = Fixture::new();
        let mut manifest = f.manifest(false);
        let root = f.root.canonicalize().unwrap();
        let outside = f.outside.canonicalize().unwrap();
        let allow = ![
            "absolute_outside",
            "prefix_sibling",
            "parent_traversal",
            "file_uri_outside",
        ]
        .contains(&id);
        let path = match id {
            "absolute_inside" | "file_uri_inside" => root.join("docs/source.md"),
            "prefix_sibling" => {
                let sibling = f.base.join("project-other");
                fs::create_dir(&sibling).unwrap();
                fs::write(sibling.join("source.md"), "OUTSIDE_AUTHORITY_SENTINEL").unwrap();
                sibling.canonicalize().unwrap().join("source.md")
            }
            "parent_traversal" => PathBuf::from("../outside/source.md"),
            "relative_inside" => PathBuf::from("docs/source.md"),
            _ => outside.join("source.md"),
        };
        if id.starts_with("authorized_") {
            manifest.project.authorized_roots = vec![if id == "authorized_relative_root" {
                PathBuf::from("../outside")
            } else {
                outside
            }];
        }
        if id.contains("file_uri") {
            manifest.sources[0].path = None;
            manifest.sources[0].locator = Some(url::Url::from_file_path(path).unwrap().to_string());
        } else {
            manifest.sources[0].path = Some(path);
        }
        let result = Locator::from_spec(&root, &manifest, &manifest.sources[0])
            .and_then(|l| l.read(&root, 1024));
        assert_eq!(result.is_ok(), allow, "{id}: {result:?}");
        if let Ok(snapshot) = result {
            assert_eq!(
                snapshot.bytes,
                if id.starts_with("authorized_") {
                    b"OUTSIDE_AUTHORITY_SENTINEL".as_slice()
                } else {
                    b"AUTHORIZED_SOURCE".as_slice()
                }
            );
        }
        assert!(
            contract["cases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|case| case["id"] == id)
        );
        println!("AWR_PATH_CASE {id}");
    }
}

#[cfg(unix)]
#[test]
fn configured_symlink_matrix_and_directory_child_policy() {
    for id in [
        "symlink_inside",
        "symlink_outside",
        "authorized_symlink",
        "dangling_symlink",
        "symlink_loop",
    ] {
        let f = Fixture::new();
        let mut manifest = f.manifest(false);
        let target = match id {
            "symlink_inside" => f.root.join("docs/source.md"),
            "dangling_symlink" => f.outside.join("missing.md"),
            "symlink_loop" => f.root.join("link.md"),
            _ => f.outside.join("source.md"),
        };
        std::os::unix::fs::symlink(target, f.root.join("link.md")).unwrap();
        manifest.sources[0].path = Some("link.md".into());
        if id == "authorized_symlink" {
            manifest.project.authorized_roots.push(f.outside.clone());
        }
        let result = Locator::from_spec(&f.root, &manifest, &manifest.sources[0])
            .and_then(|l| l.read(&f.root, 1024));
        assert_eq!(
            result.is_ok(),
            ["symlink_inside", "authorized_symlink"].contains(&id),
            "{id}: {result:?}"
        );
        println!("AWR_PATH_CASE {id}");
    }
    let f = Fixture::new();
    std::os::unix::fs::symlink(f.outside.join("source.md"), f.root.join("docs/link.md")).unwrap();
    let manifest = f.manifest(true);
    let inventory = MarkdownDirectoryAdapter
        .scan(&f.root, &manifest, &manifest.sources[0], 1024)
        .unwrap();
    assert_eq!(inventory.files.len(), 1);
    assert!(inventory.files.keys().all(|key| !key.contains("link.md")));
    println!("AWR_PATH_CASE directory_link_child");
}

#[cfg(unix)]
#[test]
fn manifest_escape_and_git_parent_traversal_are_rejected() {
    let f = Fixture::new();
    fs::create_dir(f.root.join(".awr")).unwrap();
    std::os::unix::fs::symlink(
        f.outside.join("source.md"),
        f.root.join(".awr/project.toml"),
    )
    .unwrap();
    assert!(matches!(
        Manifest::load(&f.root),
        Err(awr_core::Error::RuleViolation(_))
    ));
    println!("AWR_PATH_CASE manifest_escape");
    let mut manifest = f.manifest(false);
    manifest.sources[0].path = None;
    manifest.sources[0].locator = Some("git://HEAD:../outside/source.md".into());
    assert!(matches!(
        Locator::from_spec(&f.root, &manifest, &manifest.sources[0]),
        Err(awr_core::Error::RuleViolation(_))
    ));
    println!("AWR_PATH_CASE git_parent_traversal");
}
