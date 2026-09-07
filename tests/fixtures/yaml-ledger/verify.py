"""Feature-local intake check. Requires the built index_yaml example and PyYAML."""
import hashlib
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / "target/debug/examples/index_yaml"


def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def index(source, database, success=True):
    result = subprocess.run(
        ["rtk", "proxy", str(BINARY), str(ROOT), str(source), str(database)],
        capture_output=True, text=True, check=False,
    )
    assert (result.returncode == 0) == success, result.stdout + result.stderr
    return json.loads(result.stdout) if success else result.stderr


def connect(path):
    return sqlite3.connect(path.as_uri() + "?mode=ro", uri=True)


def rows(database, table):
    with connect(database) as conn:
        return {key: (identity, revision, json.loads(body)) for key, identity, revision, body in
                conn.execute(f"SELECT external_key,id,revision,payload_json FROM {table} WHERE active=1 ORDER BY external_key")}


def main():
    fixture = ROOT / "tests/fixtures/yaml-ledger/ledger.yaml"
    project = ROOT / "ledger/work-ledger.yaml"
    original = {path: digest(path) for path in (fixture, project)}
    (ROOT / ".local").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="yaml-intake-", dir=ROOT / ".local") as scratch:
        scratch = Path(scratch)
        database = scratch / "fixture.db"
        initial = index(fixture, database)
        before = {table: rows(database, table) for table in ("work_items", "goals", "plans", "evidence")}
        repeated = index(fixture, database)
        assert initial["source_revision"] == repeated["source_revision"] == 1
        assert initial["project_revision"] == repeated["project_revision"] == 2
        assert before == {table: rows(database, table) for table in before}
        work = before["work_items"]
        assert work["W2"][2]["raw_status"] == "waiting_for_customer"
        assert work["W2"][2]["status"] == "unknown" and initial["warnings"]
        assert len(work["W1"][2]["acceptance"]) == 2
        assert work["W1"][2]["owner"] == "fixture-author"
        assert before["plans"]["M1"][2]["kind"] == "milestone"
        assert before["plans"]["M1"][2]["acceptance"] == ["No source rewriting"]
        assert before["goals"]["G1"][2]["success_criteria"] == ["Working source intake"]
        assert all(record[2]["level"] == "unknown" and record[2]["verified_at"] is None for record in before["evidence"].values())
        with connect(database) as conn:
            assert conn.execute("SELECT to_key,required FROM edges WHERE from_key='W2' AND relation='depends_on' ORDER BY to_key").fetchall() == [("EXTERNAL-1", 0), ("W1", 1)]
            assert conn.execute("SELECT count(*) FROM edges WHERE active=1").fetchone()[0] == 5
            assert conn.execute("PRAGMA foreign_key_check").fetchall() == []

        mutable = scratch / "mutable.yaml"
        mutable.write_bytes(fixture.read_bytes())
        changed_db = scratch / "changed.db"
        index(mutable, changed_db)
        old_ids = {key: row[0] for key, row in rows(changed_db, "work_items").items()}
        mutable.write_text(mutable.read_text().replace("waiting_for_customer", "ready"))
        changed = index(mutable, changed_db)
        assert changed["source_revision"] == 2
        assert old_ids == {key: row[0] for key, row in rows(changed_db, "work_items").items()}
        assert rows(changed_db, "work_items")["W2"][2]["status"] == "ready"

        invalid = scratch / "invalid.yaml"
        for content in ("work_items: [{id: SAME}, {id: SAME}]", "work_items: {X: {id: Y}}", "work_items: [{id: W, depends_on: wrong}]"):
            invalid.write_text(content)
            index(invalid, scratch / "invalid.db", success=False)

        own_db = scratch / "project.db"
        own = index(project, own_db)
        source = yaml.safe_load(project.read_text())
        actual = rows(own_db, "work_items")
        assert set(actual) == {work["id"] for work in source["work_items"]}
        for work in source["work_items"]:
            payload = actual[work["id"]][2]
            assert payload["acceptance"] == work["acceptance"]
            assert payload["raw_status"] == work["status"]
            assert payload["source_ref"]["source_fingerprint"] == original[project]
        assert own["milestones"] == len(source["milestones"])
        with connect(own_db) as conn:
            count = conn.execute("SELECT count(*) FROM edges WHERE relation='depends_on' AND active=1").fetchone()[0]
            assert count == sum(len(work["depends_on"]) for work in source["work_items"])
        assert original == {path: digest(path) for path in original}
        print(json.dumps({"ok": True, "project_work_items": own["work_items"], "project_milestones": own["milestones"],
                          "project_dependencies": count, "project_evidence_references": own["evidence_references"],
                          "project_source_fingerprint": original[project], "idempotent": True,
                          "unknown_status_preserved": True, "source_files_unchanged": True,
                          "changed_source_preserves_identity": True, "malformed_sources_rejected": 3}, indent=2))


if __name__ == "__main__":
    main()
