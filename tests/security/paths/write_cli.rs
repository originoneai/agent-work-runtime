use awr_core::*;
use awr_store::Store;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const WORK: &str = "work_items:\n- id: W\n  title: Review source boundaries\n  status: in_progress\n  next_action: Review the draft\n  acceptance: [Preserve source authority]\n";
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let base = std::env::temp_dir().join(format!("awr-security-write-{}", Id::new()));
        fs::create_dir_all(base.join("project/.awr")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        let base = base.canonicalize().unwrap();
        let root = base.join("project");
        let outside = base.join("outside");
        let source = match mode {
            "external" => outside.join("work.yaml"),
            "runtime" => root.join(".awr/work.yaml"),
            _ => root.join("work.yaml"),
        };
        fs::write(&source, WORK).unwrap();
        fs::write(outside.join("unrelated.txt"), "OUTSIDE_UNRELATED_CONTENT").unwrap();
        let manifest = format!(
            "[project]\nname='Write boundary fixture'\nauthorized_roots=[{}]\n[[sources]]\ndomain='ledger'\nrole='primary'\npath={}\nadapter='yaml-ledger-v1'\n",
            serde_json::to_string(&outside).unwrap(),
            serde_json::to_string(&source).unwrap()
        );
        fs::write(root.join("sources.toml"), manifest).unwrap();
        let f = Self {
            base,
            root,
            outside,
            source,
        };
        f.ok(&["init", "--manifest", "sources.toml", "--accept"]);
        f
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .arg("--project")
            .arg(&self.root)
            .arg("--json")
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn revision(&self) -> String {
        self.ok(&["proposal", "list"])["project_revision"].to_string()
    }
    fn approved(&self) -> String {
        let value = self.ok(&[
            "proposal",
            "create",
            "--kind",
            "work",
            "--target",
            "W",
            "--intent",
            "Continue the reviewed source work",
            "--patch",
            r#"{"next_action":"Read the revised draft"}"#,
            "--expected-revision",
            &self.revision(),
        ]);
        let id = value["proposal"]["id"].as_str().unwrap();
        for action in ["submit", "approve"] {
            let out = self.review(action, id);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        id.into()
    }
    fn review(&self, action: &str, id: &str) -> Output {
        self.run(&[
            "proposal",
            action,
            id,
            "--actor",
            "fixture-reviewer",
            "--reason",
            "Reviewed the exact source binding",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn error(&self, output: &Output, code: &str) {
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            code
        );
    }
    fn interrupt_before_write(&self, id: &str) -> MutationApplyAttempt {
        let mut store = Store::open_existing(&self.root.join(".awr/state.db")).unwrap();
        let project = store.project_by_root(&self.root).unwrap();
        let proposal = store.proposal(project.id, id.parse().unwrap()).unwrap();
        let source = store.source(project.id, proposal.source_id).unwrap();
        let prepared = awr_source::prepare_yaml_mutation(
            &self.root,
            &source,
            &proposal,
            store.projection_ids(&source).unwrap(),
        )
        .unwrap();
        let path = self.root.join(prepared.plan.recovery_directory());
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("before.yaml"), &prepared.before.bytes).unwrap();
        fs::write(path.join("after.yaml"), &prepared.after.bytes).unwrap();
        fs::write(
            path.join("plan.json"),
            serde_json::to_vec(&prepared.plan).unwrap(),
        )
        .unwrap();
        store
            .begin_proposal_apply(
                project.id,
                project.project_revision,
                proposal.id,
                prepared.plan,
                "fixture-writer",
                "Persisted interruption boundary",
            )
            .unwrap()
            .0
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn changed_alias_to_an_authorized_equal_file_cannot_retarget_an_approved_write() {
    let f = Fixture::new("local");
    let id = f.approved();
    fs::write(f.outside.join("work.yaml"), WORK).unwrap();
    fs::remove_file(&f.source).unwrap();
    std::os::unix::fs::symlink(f.outside.join("work.yaml"), &f.source).unwrap();
    let result = f.review("apply", &id);
    f.error(&result, "SourceConflict");
    assert_eq!(
        fs::read(f.outside.join("work.yaml")).unwrap(),
        WORK.as_bytes()
    );
    assert_eq!(
        fs::read(f.outside.join("unrelated.txt")).unwrap(),
        b"OUTSIDE_UNRELATED_CONTENT"
    );
    println!("AWR_PATH_CASE source_alias_retarget");
}

#[test]
fn explicitly_authorized_external_source_is_written_with_bound_receipts() {
    let f = Fixture::new("external");
    let before = fs::read(&f.source).unwrap();
    let id = f.approved();
    let result = f.review("apply", &id);
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let body: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(body["proposal"]["status"], "applied");
    assert_eq!(body["source_write_performed"], true);
    let recovery = f.root.join(body["recovery_directory"].as_str().unwrap());
    assert_eq!(fs::read(recovery.join("before.yaml")).unwrap(), before);
    assert_eq!(
        fs::read(recovery.join("after.yaml")).unwrap(),
        fs::read(&f.source).unwrap()
    );
    assert_eq!(
        f.ok(&["work", "show", "W"])["work"]["next_action"],
        "Read the revised draft"
    );
    assert_eq!(
        fs::read(f.outside.join("unrelated.txt")).unwrap(),
        b"OUTSIDE_UNRELATED_CONTENT"
    );
    println!("AWR_PATH_CASE authorized_external_write");
}

#[cfg(unix)]
#[test]
fn recovery_snapshot_directory_alias_cannot_redirect_source_or_snapshot_writes() {
    let f = Fixture::new("local");
    let id = f.approved();
    let attempt = f.interrupt_before_write(&id);
    let recovery = f.root.join(attempt.plan.recovery_directory());
    let held = recovery.with_extension("held");
    fs::rename(&recovery, &held).unwrap();
    for name in ["before.yaml", "after.yaml", "plan.json"] {
        fs::copy(held.join(name), f.outside.join(name)).unwrap();
    }
    std::os::unix::fs::symlink(&f.outside, &recovery).unwrap();
    let before = ["before.yaml", "after.yaml", "plan.json", "unrelated.txt"]
        .map(|name| (name, fs::read(f.outside.join(name)).unwrap()));
    let result = f.review("recover", &id);
    f.error(&result, "RuleViolation");
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains(recovery.to_str().unwrap())
    );
    for (name, bytes) in before {
        assert_eq!(fs::read(f.outside.join(name)).unwrap(), bytes);
    }
    assert_eq!(fs::read(&f.source).unwrap(), WORK.as_bytes());
    println!("AWR_PATH_CASE recovery_symlink");
}

#[test]
fn runtime_owned_source_is_never_an_automatic_write_target() {
    let f = Fixture::new("runtime");
    let id = f.approved();
    let result = f.review("apply", &id);
    f.error(&result, "proposal_required");
    assert_eq!(fs::read(&f.source).unwrap(), WORK.as_bytes());
    println!("AWR_PATH_CASE runtime_source_write");
}
