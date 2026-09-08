"""Local fault-injection check: damage only an isolated disposable fixture database."""
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

binary = Path(__file__).resolve().parents[3] / "target/debug/awr"
with tempfile.TemporaryDirectory(prefix="awr-doctor-dangling-") as directory:
    root = Path(directory)
    source = "work_items:\n- id: W\n  title: Preserve an interrupted edit\n  status: in_progress\n"
    (root / "work-ledger.yaml").write_text(source)

    def run(*args):
        result = subprocess.run(
            ["rtk", "proxy", str(binary), "--project", str(root), "--json", *args],
            capture_output=True, text=True, check=False,
        )
        value = json.loads(result.stdout) if result.stdout.strip() else None
        return result, value

    result, initialized = run("init", "--accept")
    assert result.returncode == 0, result.stderr
    result, state = run("status")
    assert result.returncode == 0, result.stderr
    result, started = run("session", "start", "--work", "W", "--agent", "fixture-writer",
                          "--provider", "fixture", "--model", "fixture",
                          "--expected-revision", str(state["project_revision"]))
    assert result.returncode == 0, result.stderr
    database = root / ".awr/state.db"
    # This bypass is intentional fault injection and applies only inside TemporaryDirectory.
    with sqlite3.connect(database) as connection:
        connection.execute("PRAGMA foreign_keys=OFF")
        connection.execute("DELETE FROM work_items WHERE external_key='W'")
    result, diagnosis = run("doctor")
    assert result.returncode != 0 and diagnosis["ok"] is False
    assert diagnosis["database_ok"] is False and diagnosis["foreign_key_violations"] > 0
    assert any(f["code"] == "orphan_session" for f in diagnosis["findings"])
    assert all(f["repair"] is None for f in diagnosis["findings"])
    assert diagnosis["project_revision"] == started["project_revision"]
    result, _ = run("doctor", "repair", "interrupt-session", started["session"]["id"],
                    "--expected-revision", str(started["project_revision"]), "--reason", "Fixture-only attempt")
    assert result.returncode != 0
    with sqlite3.connect(database) as connection:
        assert connection.execute("SELECT project_revision FROM projects").fetchone()[0] == started["project_revision"]
        assert connection.execute("SELECT status FROM sessions").fetchone()[0] == "active"
    assert (root / "work-ledger.yaml").read_text() == source
    print("PASS: dangling work binding diagnosed alongside FK failures; no repair, revision change or source rewrite.")
