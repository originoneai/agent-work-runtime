use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output},
    sync::Arc,
    time::{Duration, Instant},
};
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr execution {}", awr_core::Id::new())));
        fs::create_dir(&p.0).unwrap();
        p.ok(&["init", "--goal", "Export a reviewed report", "--accept"]);
        p
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .env("AWR_EXECUTION_TEST_ROOT", &self.0)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        decode(self.run(args))
    }
    fn bind(&self, name: &str) -> String {
        self.ok(&[
            "client",
            "bind",
            "--client",
            "generic",
            "--external-session",
            name,
            "--work",
            "INTAKE-001",
        ])["binding"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    fn launch(&self, session: &str, key: &str, helper: &str) -> Value {
        self.ok(&[
            "execution",
            "run",
            "--session",
            session,
            "--key",
            key,
            "--purpose",
            "Build the report",
            "--",
            std::env::current_exe().unwrap().to_str().unwrap(),
            "--ignored",
            "--exact",
            helper,
            "--nocapture",
        ])
    }
    fn wait(&self, id: &str, states: &[&str]) -> Value {
        let start = Instant::now();
        loop {
            let e = self.ok(&["execution", "show", id])["execution"].clone();
            if states.contains(&e["state"].as_str().unwrap()) {
                return e;
            }
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "execution timed out: {e}"
            );
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn decode(o: Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
#[ignore = "owned child process fixture, launched explicitly by integration checks"]
fn successful_child() {
    let root = PathBuf::from(std::env::var_os("AWR_EXECUTION_TEST_ROOT").unwrap());
    let mut count = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("launches.txt"))
        .unwrap();
    writeln!(count, "one launch").unwrap();
    println!("report started");
    std::thread::sleep(Duration::from_millis(1500));
    fs::write(root.join("report.txt"), "Reviewed report artifact").unwrap();
    println!("report complete");
}
#[test]
#[ignore = "owned child process fixture, launched explicitly by integration checks"]
fn failed_child() {
    eprintln!("report input rejected");
    std::process::exit(7);
}

#[test]
fn managed_command_outlives_calling_session_and_keeps_result_logs() {
    let p = Project::new();
    let s = p.bind("first");
    let launched = p.launch(&s, "report-one", "successful_child");
    let id = launched["execution"]["id"].as_str().unwrap();
    assert_eq!(launched["execution"]["state"], "registered");
    assert_eq!(launched["created"], true);
    p.wait(id, &["running"]);
    let rev = p.ok(&["status"])["project_revision"].to_string();
    p.ok(&[
        "session",
        "end",
        "--session",
        &s,
        "--expected-revision",
        &rev,
    ]);
    let finished = p.wait(id, &["succeeded", "failed"]);
    assert_eq!(finished["state"], "succeeded");
    assert_eq!(finished["exit_code"], 0);
    assert!(p.0.join("report.txt").is_file());
    assert!(
        fs::read_to_string(p.0.join(finished["stdout"].as_str().unwrap()))
            .unwrap()
            .contains("report complete")
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(p.0.join(finished["receipt"].as_str().unwrap())).unwrap())
            .unwrap();
    assert_eq!(receipt["execution_id"], id);
    assert_eq!(receipt["success"], true);
    let next = p.bind("next");
    let duplicate = p.launch(&next, "report-one", "successful_child");
    assert_eq!(duplicate["created"], false);
    assert_eq!(duplicate["execution"]["id"], id);
    assert_eq!(
        fs::read_to_string(p.0.join("launches.txt"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        p.ok(&["execution", "list", "--work", "INTAKE-001"])["executions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn concurrent_registration_dispatches_only_one_child_and_rejects_changed_intent() {
    let p = Arc::new(Project::new());
    let s = p.bind("parallel");
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let p = p.clone();
            let s = s.clone();
            std::thread::spawn(move || p.launch(&s, "same-operation", "successful_child"))
        })
        .collect();
    let values: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(values.iter().filter(|v| v["created"] == true).count(), 1);
    assert!(
        values
            .iter()
            .all(|v| v["execution"]["id"] == values[0]["execution"]["id"])
    );
    p.wait(
        values[0]["execution"]["id"].as_str().unwrap(),
        &["succeeded"],
    );
    assert_eq!(
        fs::read_to_string(p.0.join("launches.txt"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let collision = p.run(&[
        "execution",
        "run",
        "--session",
        &s,
        "--key",
        "same-operation",
        "--purpose",
        "Different intent",
        "--",
        "different-command",
    ]);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("SourceConflict"));
}
#[test]
fn failures_and_spawn_errors_have_durable_terminal_receipts() {
    let p = Project::new();
    let s = p.bind("failure");
    let e = p.launch(&s, "failed-report", "failed_child");
    let failed = p.wait(e["execution"]["id"].as_str().unwrap(), &["failed"]);
    assert_eq!(failed["exit_code"], 7);
    assert!(
        fs::read_to_string(p.0.join(failed["stderr"].as_str().unwrap()))
            .unwrap()
            .contains("input rejected")
    );
    let e = p.ok(&[
        "execution",
        "run",
        "--session",
        &s,
        "--key",
        "missing-tool",
        "--purpose",
        "Build report",
        "--",
        "awr-fixture-command-that-does-not-exist",
    ]);
    let failed = p.wait(e["execution"]["id"].as_str().unwrap(), &["failed"]);
    assert!(failed["exit_code"].is_null());
    assert!(failed["error"].as_str().unwrap().contains("spawn failed"));
}
#[test]
fn external_reference_is_not_a_completion_and_generic_events_cannot_forge_it() {
    let p = Project::new();
    let s = p.bind("external");
    let e = p.ok(&[
        "execution",
        "register",
        "--session",
        &s,
        "--key",
        "external-build",
        "--purpose",
        "Track build",
        "--reference",
        "ci://build/42",
    ]);
    assert_eq!(e["execution"]["state"], "registered");
    assert_eq!(e["outcome_verified"], false);
    assert!(e["execution"]["worker"].is_null());
    let rev = p.ok(&["status"])["project_revision"].to_string();
    let forged = p.run(&[
        "event",
        "append",
        "--type",
        "execution.finished",
        "--summary",
        "Pretend completed",
        "--expected-revision",
        &rev,
    ]);
    assert!(!forged.status.success());
    assert!(String::from_utf8_lossy(&forged.stderr).contains("reserved"));
}
