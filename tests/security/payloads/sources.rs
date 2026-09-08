use awr_core::{EntityKind, Error, Freshness, Id};
use awr_source::*;
use awr_store::Store;
use std::{fs, path::PathBuf, process::Command};

const MARKDOWN_CAP: usize = 2 * 1024 * 1024;
const YAML_CAP: usize = 4 * 1024 * 1024;

struct Fixture {
    root: PathBuf,
    manifest: Manifest,
    store: Store,
}
impl Fixture {
    fn new(adapter: &str) -> Self {
        let root = std::env::temp_dir().join(format!("awr-payload-source-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let (domain, path) = match adapter {
            "yaml-ledger-v1" => ("ledger", "work.yaml"),
            "markdown-directory-v1" => ("decisions", "decisions"),
            _ => ("goal", "goals.md"),
        };
        let manifest = Manifest::parse(&format!(
            "[project]\nname='Bounded source fixture'\n[[sources]]\ndomain='{domain}'\nrole='primary'\npath='{path}'\nadapter='{adapter}'\n"
        )).unwrap();
        let store = Store::open(&root.join("state.db")).unwrap();
        Self {
            root,
            manifest,
            store,
        }
    }
    fn path(&self) -> PathBuf {
        self.root
            .join(self.manifest.sources[0].path.as_ref().unwrap())
    }
    fn bytes(&self, size: usize) -> Vec<u8> {
        let mut bytes = if self.manifest.sources[0].adapter == "yaml-ledger-v1" {
            b"work_items:\n- id: W\n  title: Retained work\n  status: ready\n  next_action: Read the report\n#".to_vec()
        } else {
            b"# Retained goal {status=active}\n".to_vec()
        };
        assert!(size >= bytes.len());
        bytes.resize(size, b'x');
        bytes
    }
    fn index(&mut self) -> IndexReport {
        index_project(&mut self.store, &self.root, &self.manifest, false).unwrap()
    }
    fn rejected(&mut self) -> IndexReport {
        let report = self.index();
        assert!(!report.ok, "oversized source was indexed");
        assert_eq!(report.indexed, 0);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.code == "InvalidInput" && i.message.contains("cap"))
        );
        assert!(report.issues.iter().all(|i| i.locator.is_some()));
        report
    }
    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn file_adapters_allow_the_exact_byte_limit_and_refuse_one_more_byte() {
    for (adapter, cap, id) in [
        ("markdown-heading-v1", MARKDOWN_CAP, "markdown_limit"),
        ("yaml-ledger-v1", YAML_CAP, "yaml_limit"),
    ] {
        let mut f = Fixture::new(adapter);
        fs::write(f.path(), f.bytes(cap)).unwrap();
        assert!(f.index().ok);
        fs::write(f.path(), f.bytes(cap + 1)).unwrap();
        f.rejected();
        println!("AWR_PAYLOAD_CASE {id}");
    }
}

#[test]
fn direct_adapter_parsing_cannot_bypass_the_byte_limit() {
    for (adapter, cap) in [
        ("markdown-heading-v1", MARKDOWN_CAP),
        ("yaml-ledger-v1", YAML_CAP),
    ] {
        let mut f = Fixture::new(adapter);
        fs::write(f.path(), f.bytes(200)).unwrap();
        let report = f.index();
        assert!(report.ok);
        let source = f
            .store
            .source(report.project_id, report.sources[0].source_id)
            .unwrap();
        let bytes = f.bytes(cap + 1);
        let snapshot = SourceSnapshot {
            locator: source.locator.clone(),
            fingerprint: fingerprint(&bytes),
            bytes,
        };
        let context = ParseContext {
            source: &source,
            existing_ids: Default::default(),
        };
        assert!(
            matches!(source_adapter(adapter).unwrap().parse(&snapshot, &context, &f.manifest.sources[0]), Err(Error::InvalidInput(message)) if message.contains("cap"))
        );
    }
    println!("AWR_PAYLOAD_CASE direct_parser_limit");
}

#[test]
fn oversized_refresh_retains_last_known_projection_and_marks_it_unavailable() {
    let mut f = Fixture::new("yaml-ledger-v1");
    fs::write(f.path(), f.bytes(200)).unwrap();
    let first = f.index();
    assert!(first.ok);
    let source = f
        .store
        .source(first.project_id, first.sources[0].source_id)
        .unwrap();
    let old = f
        .store
        .source_projection_payloads(&source, EntityKind::WorkItem)
        .unwrap();
    let bytes = String::from_utf8(f.bytes(YAML_CAP + 100))
        .unwrap()
        .replace("Retained work", "Unexpected work");
    fs::write(f.path(), bytes).unwrap();
    f.rejected();
    let current = f.store.source(first.project_id, source.id).unwrap();
    assert_eq!(current.freshness, Freshness::Unavailable);
    assert_eq!(current.fingerprint, source.fingerprint);
    assert_eq!(
        f.store
            .source_projection_payloads(&current, EntityKind::WorkItem)
            .unwrap(),
        old
    );
    println!("AWR_PAYLOAD_CASE source_refresh_limit");
}

#[test]
fn git_backed_sources_apply_the_same_adapter_limit() {
    for (adapter, cap) in [
        ("markdown-heading-v1", MARKDOWN_CAP),
        ("yaml-ledger-v1", YAML_CAP),
    ] {
        let mut f = Fixture::new(adapter);
        let name = f.manifest.sources[0].path.take().unwrap();
        fs::write(f.root.join(&name), f.bytes(cap + 1)).unwrap();
        f.git(&["init", "-q"]);
        f.git(&["add", name.to_str().unwrap()]);
        f.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "Bounded fixture",
        ]);
        f.manifest.sources[0].locator = Some(format!("git://HEAD:{}", name.display()));
        f.rejected();
    }
    println!("AWR_PAYLOAD_CASE git_source_limit");
}

#[test]
fn directory_inventory_and_indexing_enforce_the_markdown_limit() {
    let mut f = Fixture::new("markdown-directory-v1");
    fs::create_dir(f.path()).unwrap();
    fs::write(f.path().join("decision.md"), f.bytes(MARKDOWN_CAP + 1)).unwrap();
    assert!(
        matches!(MarkdownDirectoryAdapter.scan(&f.root, &f.manifest, &f.manifest.sources[0], 16 * 1024 * 1024), Err(Error::InvalidInput(message)) if message.contains("cap"))
    );
    f.rejected();
    println!("AWR_PAYLOAD_CASE directory_source_limit");
}

#[test]
fn source_readers_reject_zero_and_overflow_limits() {
    let f = Fixture::new("yaml-ledger-v1");
    fs::write(f.path(), []).unwrap();
    for cap in [0, u64::MAX] {
        assert!(matches!(
            read_capped(&f.path(), cap),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            read_source_capped(&f.path(), cap),
            Err(Error::InvalidInput(_))
        ));
    }
    println!("AWR_PAYLOAD_CASE reader_limit_validation");
}
