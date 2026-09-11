use awr_core::{EventDraft, Id};
use awr_store::Store;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Fixture(PathBuf);
fn copied_program(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "backup/program/awr.exe"
    } else {
        "backup/program/awr"
    })
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!("awr-snapshot-{}", Id::new())));
        fs::create_dir(&f.0).unwrap();
        fs::write(f.0.join("work.yaml"),"goals:\n- id: G\n  title: Deliver a useful guide\n  status: active\nwork_items:\n- id: W\n  title: Draft a guide\n  status: in_progress\n  goal: G\n  acceptance: [Useful examples]\n  next_action: Review examples\n").unwrap();
        fs::write(f.0.join("map.toml"),"[project]\nname='Guide'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        f.ok(&["init", "--manifest", "map.toml", "--accept"]);
        fs::write(
            f.0.join(".awr/runtime-binding.json"),
            "{\"host\":\"synthetic-host\",\"version\":1}\n",
        )
        .unwrap();
        f
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let r = self.run(args);
        assert!(
            r.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn reject(&self, args: &[&str]) -> String {
        let r = self.run(args);
        assert!(!r.status.success(), "unexpected success {args:?}");
        String::from_utf8(r.stderr).unwrap()
    }
    fn db(&self) -> PathBuf {
        self.0.join(".awr/state.db")
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn advance(&self) {
        let mut s = Store::open_existing(&self.db()).unwrap();
        let p = s.project_by_root(&self.0).unwrap();
        s.append_event(
            p.id,
            p.project_revision,
            EventDraft::new("work.progress", "Review additional examples"),
        )
        .unwrap();
    }
    fn backup(&self) -> Value {
        self.ok(&["runtime", "backup", "--output", "backup"])
    }
    fn preview(&self) -> Value {
        self.ok(&["runtime", "restore-preview", "--backup", "backup"])
    }
    fn restore(&self, p: &Value) -> Value {
        self.ok(&[
            "runtime",
            "restore",
            "--backup",
            "backup",
            "--expected-preview",
            p["fingerprint"].as_str().unwrap(),
            "--offline",
        ])
    }
}
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, p: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    fs::read(p).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
#[test]
fn backup_and_matching_restore_preserve_history_identity_and_new_user_files() {
    let f = Fixture::new();
    let session = f.ok(&[
        "session",
        "start",
        "--work",
        "W",
        "--agent",
        "writer",
        "--provider",
        "fixture",
        "--model",
        "test",
        "--expected-revision",
        &f.revision(),
    ]);
    let sid = session["session"]["id"].as_str().unwrap();
    let context = f.ok(&["context", "compile", "--session", sid, "--work", "W"]);
    assert!(
        context["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("Useful examples")
    );
    let hash = context["work_context"]["context_hash"].as_str().unwrap();
    let checkpoint = f.ok(&[
        "session",
        "checkpoint",
        "--session",
        sid,
        "--context-hash",
        hash,
        "--digest",
        "Draft reviewed",
        "--next-action",
        "Revise the final example",
        "--expected-revision",
        &f.revision(),
    ]);
    fs::write(f.0.join("report.txt"), "Example review completed").unwrap();
    let evidence = json!({"external_key":"GUIDE-REVIEW","work_item_key":"W","evidence_type":"report","level":"locally_verified","summary":"Reviewed the guide","locator":"report.txt","sha256":awr_source::fingerprint(b"Example review completed").trim_start_matches("sha256:"),"source_sha":"a".repeat(40),"command":"review guide","scope":["W"],"branch_id":null,"verified_at":1});
    fs::write(
        f.0.join("evidence.json"),
        serde_json::to_vec(&evidence).unwrap(),
    )
    .unwrap();
    let added = f.ok(&[
        "evidence",
        "add",
        "--input",
        "evidence.json",
        "--expected-revision",
        &f.revision(),
    ]);
    let before = tree(&f.0.join(".awr"));
    let binding = f.ok(&["runtime", "binding"]);
    let backup = f.backup();
    let retained = Command::new(copied_program(&f.0))
        .args([
            "--json",
            "runtime",
            "check",
            "--backup",
            f.0.join("backup").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        retained.status.success(),
        "{}",
        String::from_utf8_lossy(&retained.stderr)
    );
    assert_eq!(tree(&f.0.join(".awr")), before);
    assert_eq!(backup["snapshot"]["project_id"], binding["project"]["id"]);
    assert_eq!(
        f.ok(&["runtime", "check", "--backup", "backup"])["phase"],
        "verified"
    );
    f.advance();
    fs::write(f.0.join("new-user-file.txt"), "Keep the newer draft").unwrap();
    fs::write(
        f.0.join(".awr/new-host-note.json"),
        "{\"note\":\"keep me\"}",
    )
    .unwrap();
    let before_preview = tree(&f.0.join(".awr"));
    let plan = f.preview();
    assert_eq!(tree(&f.0.join(".awr")), before_preview);
    assert_eq!(plan["history_after_restore_revision_will_be_removed"], true);
    assert!(
        plan["preserved_additional_runtime_files"]
            .get("new-host-note.json")
            .is_some()
    );
    let result = f.restore(&plan);
    assert_eq!(
        result["project_revision"],
        backup["snapshot"]["project_revision"]
    );
    assert_eq!(
        fs::read_to_string(f.0.join("new-user-file.txt")).unwrap(),
        "Keep the newer draft"
    );
    assert_eq!(
        fs::read(f.0.join(".awr/new-host-note.json")).unwrap(),
        before_preview["new-host-note.json"]
    );
    assert_eq!(
        f.ok(&["session", "show", sid])["session"]["id"],
        session["session"]["id"]
    );
    let cp = f.ok(&[
        "object",
        "show",
        "checkpoint",
        checkpoint["checkpoint"]["id"].as_str().unwrap(),
        "--full",
    ]);
    assert!(cp.to_string().contains("Draft reviewed"));
    assert_eq!(
        f.ok(&["evidence", "show", "GUIDE-REVIEW"])["evidence"]["id"],
        added["evidence"]["id"]
    );
    assert_eq!(f.ok(&["runtime", "restore-status"])["pending"], false);
    assert!(
        Path::new(result["rollback"].as_str().unwrap())
            .join("state.db")
            .is_file()
    );
}
#[test]
fn source_configuration_auxiliary_drift_and_stale_previews_never_overwrite() {
    let f = Fixture::new();
    f.backup();
    f.advance();
    let plan = f.preview();
    f.advance();
    let db = fs::read(f.db()).unwrap();
    assert!(
        f.reject(&[
            "runtime",
            "restore",
            "--backup",
            "backup",
            "--expected-preview",
            plan["fingerprint"].as_str().unwrap(),
            "--offline"
        ])
        .contains("stale")
    );
    assert_eq!(fs::read(f.db()).unwrap(), db);
    for name in [
        "work.yaml",
        ".awr/project.toml",
        ".awr/runtime-binding.json",
    ] {
        let path = f.0.join(name);
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n# New user change\n");
        fs::write(&path, &changed).unwrap();
        f.reject(&["runtime", "restore-preview", "--backup", "backup"]);
        assert_eq!(fs::read(&path).unwrap(), changed);
        assert_eq!(fs::read(f.db()).unwrap(), db);
        fs::write(path, original).unwrap();
    }
    let program = copied_program(&f.0);
    let bytes = fs::read(&program).unwrap();
    fs::write(&program, b"altered").unwrap();
    f.reject(&["runtime", "check", "--backup", "backup"]);
    fs::write(program, bytes).unwrap();
    assert!(!f.0.join(".awr/state.db-restore.pending").exists());
    let other = Fixture::new();
    other.reject(&[
        "runtime",
        "restore-preview",
        "--backup",
        f.0.join("backup").to_str().unwrap(),
    ]);
}
#[test]
fn offline_restore_requires_closed_clients_and_matching_program_bytes() {
    let f = Fixture::new();
    f.backup();
    f.advance();
    let p = f.preview();
    f.reject(&[
        "runtime",
        "restore",
        "--backup",
        "backup",
        "--expected-preview",
        p["fingerprint"].as_str().unwrap(),
    ]);
    let held = Store::open_existing(&f.db()).unwrap();
    assert!(
        f.reject(&[
            "runtime",
            "restore",
            "--backup",
            "backup",
            "--expected-preview",
            p["fingerprint"].as_str().unwrap(),
            "--offline"
        ])
        .contains("in use")
    );
    drop(held);
    // A resealed manifest with an altered program still fails target matching.
    let path = f.0.join("backup/snapshot.json");
    let original = fs::read(&path).unwrap();
    let mut s: Value = serde_json::from_slice(&original).unwrap();
    let mut binary = fs::read(copied_program(&f.0)).unwrap();
    binary.push(0);
    fs::write(copied_program(&f.0), &binary).unwrap();
    s["program"]["binary"]["sha256"] = json!(awr_source::fingerprint(&binary));
    s["program"]["binary"]["bytes"] = json!(binary.len());
    s["fingerprint"] = json!("");
    s["fingerprint"] = json!(awr_source::fingerprint(&serde_json::to_vec(&s).unwrap()));
    fs::write(&path, serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(
        f.ok(&["runtime", "check", "--backup", "backup"])["phase"],
        "verified"
    );
    assert!(
        f.reject(&["runtime", "restore-preview", "--backup", "backup"])
            .contains("executing CLI differs")
    );
    binary.pop();
    fs::write(copied_program(&f.0), binary).unwrap();
    fs::write(path, original).unwrap();
    f.restore(&f.preview());
}
#[test]
fn interrupted_restore_can_resume_before_or_after_install_but_refuses_external_drift() {
    let f = Fixture::new();
    f.backup();
    f.advance();
    let result = f.restore(&f.preview());
    let path = PathBuf::from(result["receipt"].as_str().unwrap());
    let dir = path.parent().unwrap();
    let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt["phase"] = json!("prepared");
    // Reconstruct an interruption immediately after the durable marker and before replacement.
    fs::copy(dir.join("rollback/state.db"), f.db()).unwrap();
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    fs::write(
        f.0.join(".awr/state.db-restore.pending"),
        result["restore_id"].as_str().unwrap(),
    )
    .unwrap();
    assert!(f.reject(&["session", "list"]).contains("restore"));
    assert_eq!(
        f.ok(&["runtime", "restore-status"])["receipt"]["phase"],
        "prepared"
    );
    let source = f.0.join("work.yaml");
    let before = fs::read(&source).unwrap();
    fs::write(
        &source,
        [before.clone(), b"# User edit\n".to_vec()].concat(),
    )
    .unwrap();
    f.reject(&["runtime", "restore-recover", "--offline"]);
    assert!(f.0.join(".awr/state.db-restore.pending").exists());
    fs::write(source, before).unwrap();
    let good_db = fs::read(f.db()).unwrap();
    fs::write(f.db(), b"External database replacement").unwrap();
    assert!(
        f.reject(&["runtime", "restore-recover", "--offline"])
            .contains("outside the recorded restore")
    );
    fs::write(f.db(), good_db).unwrap();
    let recovered = f.ok(&["runtime", "restore-recover", "--offline"]);
    assert_eq!(recovered["project_revision"], result["project_revision"]);
    // Interruption after completed receipt, before marker removal: only finalize, never rewind again.
    fs::write(
        f.0.join(".awr/state.db-restore.pending"),
        result["restore_id"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        f.ok(&["runtime", "restore-recover", "--offline"])["project_revision"],
        result["project_revision"]
    );
    f.advance();
    fs::write(
        f.0.join(".awr/state.db-restore.pending"),
        result["restore_id"].as_str().unwrap(),
    )
    .unwrap();
    f.reject(&["runtime", "restore-recover", "--offline"]);
}
#[test]
fn missing_database_can_restore_but_existing_unknown_database_is_never_overwritten() {
    let f = Fixture::new();
    let backup = f.backup();
    let unknown = b"An unrelated or corrupt database";
    fs::write(f.db(), unknown).unwrap();
    assert!(
        f.reject(&["runtime", "restore-preview", "--backup", "backup"])
            .contains("ownership")
    );
    assert_eq!(fs::read(f.db()).unwrap(), unknown);
    fs::remove_file(f.db()).unwrap();
    let plan = f.preview();
    assert!(plan["current_revision"].is_null());
    assert_eq!(
        f.restore(&plan)["project_id"],
        backup["snapshot"]["project_id"]
    );
}
#[test]
fn unregistered_directory_additions_and_payload_traversal_are_rejected() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("notes")).unwrap();
    fs::write(f.0.join("notes/one.md"), "# A note\nInitial note\n").unwrap();
    let path = f.0.join(".awr/project.toml");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='notes'\nadapter='markdown-directory-v1'\n");
    fs::write(path, text).unwrap();
    f.ok(&["source", "reindex"]);
    f.backup();
    fs::write(f.0.join("notes/two.md"), "# Another note\nAdded later\n").unwrap();
    f.reject(&["runtime", "restore-preview", "--backup", "backup"]);
    let path = f.0.join("backup/snapshot.json");
    let mut s: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    s["database"]["path"] = json!("../../.awr/state.db");
    s["fingerprint"] = json!("");
    s["fingerprint"] = json!(awr_source::fingerprint(&serde_json::to_vec(&s).unwrap()));
    fs::write(path, serde_json::to_vec(&s).unwrap()).unwrap();
    f.reject(&["runtime", "check", "--backup", "backup"]);
}

#[test]
fn backup_includes_committed_wal_and_restore_retains_displaced_sidecars() {
    let f = Fixture::new();
    let mut held = Store::open_existing(&f.db()).unwrap();
    let p = held.project_by_root(&f.0).unwrap();
    let event = held
        .append_event(
            p.id,
            p.project_revision,
            EventDraft::new("work.progress", "Committed in the WAL"),
        )
        .unwrap();
    assert!(f.0.join(".awr/state.db-wal").metadata().unwrap().len() > 0);
    let backup = f.backup();
    assert_eq!(
        backup["snapshot"]["project_revision"],
        event.project_revision
    );
    let later = held
        .append_event(
            p.id,
            event.project_revision,
            EventDraft::new("work.progress", "Later WAL history"),
        )
        .unwrap();
    let names = ["state.db", "state.db-wal", "state.db-shm"];
    let displaced: Vec<_> = names
        .iter()
        .map(|n| fs::read(f.0.join(".awr").join(n)).unwrap())
        .collect();
    drop(held);
    // Reproduce files left by a stopped writer without checkpointing its WAL into the main file.
    for (name, bytes) in names.iter().zip(&displaced) {
        fs::write(f.0.join(".awr").join(name), bytes).unwrap();
    }
    let plan = f.preview();
    assert_eq!(plan["current_revision"], later.project_revision);
    let restored = f.restore(&plan);
    assert_eq!(restored["project_revision"], event.project_revision);
    let rollback = Path::new(restored["rollback"].as_str().unwrap());
    for (name, bytes) in names.iter().zip(&displaced) {
        assert_eq!(fs::read(rollback.join(name)).unwrap(), *bytes);
    }
    assert!(!f.0.join(".awr/state.db-wal").exists());
    assert_eq!(
        f.ok(&["runtime", "binding"])["project"]["project_revision"],
        event.project_revision
    );
}
