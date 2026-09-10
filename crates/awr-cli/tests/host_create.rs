//! Native CLI creation/retry checks; recovery boundary edits are synthetic fixtures.
use awr_core::Id;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::Arc,
};

struct Host(PathBuf);
impl Host {
    fn new(text: &str, options: &str) -> Self {
        let h = Self(std::env::temp_dir().join(format!("awr 新增 台账 {}", Id::new())));
        fs::create_dir(&h.0).unwrap();
        h.write("工作.yaml", text);
        h.write("mapping.toml",&format!("[project]\nname='Creation'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='工作.yaml'\nadapter='yaml-ledger-v1'\n{options}"));
        h.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        h
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .env_clear()
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn error(&self, args: &[&str], code: &str) {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
            code
        );
    }
    fn preview(&self, key: &str, title: &str) -> Value {
        self.ok(&["work", "create", "--request-key", key, "--title", title])
    }
    fn accept(&self, key: &str, title: &str, preview: &Value) -> Value {
        self.ok(&[
            "work",
            "create",
            "--request-key",
            key,
            "--title",
            title,
            "--accept",
            "--expected-preview",
            preview["preview"]["fingerprint"].as_str().unwrap(),
            "--expected-revision",
            &preview["project_revision"].to_string(),
        ])
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn stored(&self, created: &Value) -> (PathBuf, Value) {
        let p = self
            .0
            .join(created["recovery_directory"].as_str().unwrap())
            .join("receipt.json");
        let value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        (p, value)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn title_only_creates_one_non_executable_draft_and_replays_the_same_identity() {
    let h = Host::new("work_items: []\n", "");
    let before = fs::read(h.0.join("工作.yaml")).unwrap();
    let p = h.preview("one-request", "准备旅行清单");
    assert_eq!(p["source_write_performed"], false);
    assert_eq!(fs::read(h.0.join("工作.yaml")).unwrap(), before);
    let saved = h.accept("one-request", "准备旅行清单", &p);
    assert_eq!(saved["phase"], "completed");
    let rev = h.revision();
    let after = fs::read(h.0.join("工作.yaml")).unwrap();
    let replay = h.accept("one-request", "准备旅行清单", &p);
    assert_eq!(saved["work_id"], replay["work_id"]);
    assert_eq!(replay["already_recorded"], true);
    assert_eq!(replay["source_write_performed"], false);
    assert_eq!(h.revision(), rev);
    assert_eq!(fs::read(h.0.join("工作.yaml")).unwrap(), after);
    let key = saved["external_key"].as_str().unwrap();
    let work = h.ok(&["work", "show", key]);
    assert_eq!(work["work"]["status"], "draft");
    assert_eq!(work["work"]["ready"], false);
    h.error(
        &[
            "session",
            "start",
            "--work",
            key,
            "--agent",
            "fixture",
            "--provider",
            "fixture",
            "--model",
            "no-model",
            "--claim",
            "--expected-revision",
            &h.revision(),
        ],
        "DependencyBlocked",
    );
    assert_eq!(h.ok(&["object", "list", "work"])["total"], 1);
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    let status = h.ok(&["work", "create-status", "--key", "one-request"]);
    assert_eq!(status["work_id"], saved["work_id"]);
    assert_eq!(status["current_source"], "after");
    assert_eq!(h.revision(), rev);
    h.error(
        &[
            "work",
            "create",
            "--request-key",
            "one-request",
            "--title",
            "Different title",
            "--accept",
        ],
        "SourceConflict",
    );
}

#[test]
fn list_map_flow_mapped_fields_and_crlf_retain_existing_records_and_ids() {
    for text in [
        "# header\nwork_items:\n- id: OLD\n  title: 'Original' # keep\n  status: planned\n# footer\n",
        "work_items: [{id: OLD, title: 'Original', status: planned}] # keep\n",
        "work_items:\r\n  OLD:\r\n    title: 'Original' # keep\r\n    status: planned\r\n# footer\r\n",
        "work_items: {OLD: {title: Original, status: planned}} # keep\n",
    ] {
        let h = Host::new(text, "");
        let old = h.ok(&["work", "show", "OLD"])["work"]["id"].clone();
        let p = h.preview("new-work", "A new work");
        h.accept("new-work", "A new work", &p);
        assert_eq!(h.ok(&["work", "show", "OLD"])["work"]["id"], old);
        let output = fs::read_to_string(h.0.join("工作.yaml")).unwrap();
        assert!(output.contains("# keep"));
        if text.contains("\r\n") {
            assert!(!output.replace("\r\n", "").contains('\n'));
        }
        let prior: Value = serde_yaml_ng::from_str(text).unwrap();
        let after: Value = serde_yaml_ng::from_str(&output).unwrap();
        if prior["work_items"].is_array() {
            assert_eq!(prior["work_items"][0], after["work_items"][0]);
        } else {
            assert_eq!(prior["work_items"]["OLD"], after["work_items"]["OLD"]);
        }
    }
    let h = Host::new(
        "work_items: []\n",
        "[sources.options.field_map]\nid='ticket'\ntitle='name'\nstatus='phase'\n[sources.options.status_map]\nDrafting='draft'\nPending='planned'\n",
    );
    let p = h.preview("mapped", "映射字段");
    let saved = h.accept("mapped", "映射字段", &p);
    let text = fs::read_to_string(h.0.join("工作.yaml")).unwrap();
    let value: Value = serde_yaml_ng::from_str(&text).unwrap();
    assert_eq!(value["work_items"][0]["phase"], "Drafting");
    assert_eq!(value["work_items"][0]["name"], "映射字段");
    assert_eq!(value["work_items"][0]["ticket"], saved["external_key"]);
    assert!(value["work_items"][0].get("id").is_none());
}

#[test]
fn changed_source_or_mapping_invalidates_review_and_missing_acceptance_never_writes() {
    for mapping in [false, true] {
        let h = Host::new("work_items: []\n", "");
        let p = h.preview("review", "New task");
        let path = if mapping {
            ".awr/project.toml"
        } else {
            "工作.yaml"
        };
        let old = fs::read_to_string(h.0.join(path)).unwrap();
        h.write(
            path,
            &if mapping {
                old.replace("role = \"primary\"", "role = \"supporting\"")
            } else {
                format!("{old}# external edit\n")
            },
        );
        let before = fs::read(h.0.join("工作.yaml")).unwrap();
        let out = h.run(&[
            "work",
            "create",
            "--request-key",
            "review",
            "--title",
            "New task",
            "--accept",
            "--expected-preview",
            p["preview"]["fingerprint"].as_str().unwrap(),
            "--expected-revision",
            &p["project_revision"].to_string(),
        ]);
        assert!(!out.status.success());
        assert_eq!(fs::read(h.0.join("工作.yaml")).unwrap(), before);
    }
    let h = Host::new("work_items: []\n", "");
    h.error(
        &[
            "work",
            "create",
            "--request-key",
            "missing-review",
            "--title",
            "New task",
            "--accept",
        ],
        "SourceConflict",
    );
    assert_eq!(
        fs::read_to_string(h.0.join("工作.yaml")).unwrap(),
        "work_items: []\n"
    );
}

#[test]
fn concurrent_delivery_and_discarded_response_do_not_create_two_work_keys() {
    let h = Arc::new(Host::new("work_items: []\n", ""));
    let p = h.preview("parallel", "Parallel task");
    let jobs = (0..4)
        .map(|_| {
            let h = h.clone();
            let p = p.clone();
            std::thread::spawn(move || {
                h.run(&[
                    "work",
                    "create",
                    "--request-key",
                    "parallel",
                    "--title",
                    "Parallel task",
                    "--accept",
                    "--expected-preview",
                    p["preview"]["fingerprint"].as_str().unwrap(),
                    "--expected-revision",
                    &p["project_revision"].to_string(),
                ])
            })
        })
        .collect::<Vec<_>>();
    let outputs = jobs
        .into_iter()
        .map(|j| j.join().unwrap())
        .collect::<Vec<_>>();
    assert!(outputs.iter().any(|o| o.status.success()));
    // The host lost/discarded successful command output. Recover through the request key.
    let status = h.ok(&["work", "create-status", "--key", "parallel"]);
    assert_eq!(status["phase"], "completed");
    let replay = h.accept("parallel", "Parallel task", &p);
    assert_eq!(replay["work_id"], status["work_id"]);
    assert_eq!(h.ok(&["object", "list", "work"])["total"], 1);
}

#[test]
fn persisted_boundary_recovery_never_overwrites_external_edits_or_allocates_a_second_id() {
    let h = Host::new("work_items: []\n", "");
    let p = h.preview("recover", "Recover task");
    let saved = h.accept("recover", "Recover task", &p);
    let after = fs::read(h.0.join("工作.yaml")).unwrap();
    let (path, mut receipt) = h.stored(&saved);
    // Synthetic boundary fixture: source installed, acknowledgement not persisted.
    receipt["phase"] = json!("applied");
    receipt["work_id"] = Value::Null;
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let recovered = h.ok(&[
        "work",
        "create-recover",
        "--key",
        "recover",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(recovered["work_id"], saved["work_id"]);
    assert_eq!(recovered["source_write_performed"], false);
    assert_eq!(fs::read(h.0.join("工作.yaml")).unwrap(), after);
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let external = [after, b"# user added new information\n".to_vec()].concat();
    fs::write(h.0.join("工作.yaml"), &external).unwrap();
    h.error(
        &[
            "work",
            "create-recover",
            "--key",
            "recover",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    assert_eq!(fs::read(h.0.join("工作.yaml")).unwrap(), external);
}

#[test]
fn unsupported_shapes_and_foreign_receipts_do_not_become_task_writes() {
    let h = Host::new("work_items: &works []\nextra: *works\n", "");
    h.error(
        &[
            "work",
            "create",
            "--request-key",
            "unsupported",
            "--title",
            "New task",
        ],
        "MutationUnsupported",
    );
    let first = Host::new("work_items: []\n", "");
    let p = first.preview("same-key", "First project");
    let saved = first.accept("same-key", "First project", &p);
    let second = Host::new("work_items: []\n", "");
    let p = second.preview("same-key", "Second project");
    let saved2 = second.accept("same-key", "Second project", &p);
    assert_ne!(saved["external_key"], saved2["external_key"]);
    assert_ne!(saved["project_id"], saved2["project_id"]);
    assert_eq!(
        second.ok(&["work", "create-status", "--key", "same-key"])["work_id"],
        saved2["work_id"]
    );
    let (path2, _) = second.stored(&saved2);
    let (path1, _) = first.stored(&saved);
    fs::copy(path1, path2).unwrap();
    second.error(
        &["work", "create-status", "--key", "same-key"],
        "SourceConflict",
    );
}

#[test]
fn old_explicit_draft_spelling_mapping_is_retained_but_cannot_make_new_work_executable() {
    let h = Host::new(
        "work_items: [{id: OLD, title: Existing work, status: draft}]\n",
        "[sources.options.status_map]\ndraft='planned'\n",
    );
    assert_eq!(h.ok(&["work", "show", "OLD"])["work"]["status"], "planned");
    h.error(
        &[
            "work",
            "create",
            "--request-key",
            "new",
            "--title",
            "New task",
        ],
        "MutationUnsupported",
    );
    assert_eq!(h.ok(&["object", "list", "work"])["total"], 1);
}
