#!/usr/bin/env python3
"""Reproducible, isolated host upgrade/rollback drill. Never targets a real user project."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def native(program, root, *args, success=True):
    p = subprocess.run([str(program), "--project", str(root), "--json", *map(str, args)],
                       env={}, capture_output=True, text=True, timeout=60)
    if success and p.returncode:
        raise AssertionError(f"{args}: {p.stdout}\n{p.stderr}")
    if not success:
        assert p.returncode, "old program unexpectedly accepted a newer schema"
        return dict(exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr)
    return json.loads(p.stdout)


def db_rows(root):
    with sqlite3.connect(f"{(root / '.awr/state.db').as_uri()}?mode=ro", uri=True) as db:
        assert db.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        tables = ["projects", "sources", "work_items", "sessions", "claims", "events", "checkpoints", "evidence", "artifacts"]
        return {t: db.execute(f'SELECT * FROM "{t}" ORDER BY id').fetchall() for t in tables}


def snapshot(root, program, out):
    """The fixture is quiescent. Retain all runtime files, but use SQLite's backup API for the database."""
    out.mkdir()
    shutil.copytree(root / ".awr", out / "runtime", ignore=shutil.ignore_patterns("state.db", "state.db-wal", "state.db-shm"))
    with sqlite3.connect(f"{(root / '.awr/state.db').as_uri()}?mode=ro", uri=True) as source:
        with sqlite3.connect(out / "runtime/state.db") as target:
            source.backup(target)
            assert target.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
            assert target.execute("PRAGMA foreign_key_check").fetchall() == []
            schema = target.execute("PRAGMA user_version").fetchone()[0]
    (out / "sources").mkdir()
    for name in ("work.yaml", "GOAL.md"):
        shutil.copy2(root / name, out / "sources" / name)
    (out / "bin").mkdir()
    shutil.copy2(program, out / "bin" / program.name)
    files = {str(p.relative_to(out)): digest(p) for p in sorted(out.rglob("*")) if p.is_file()}
    manifest = dict(version=1, root=str(root), schema=schema, program=str(Path("bin") / program.name), files=files)
    (out / "snapshot.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def verify_snapshot(directory):
    m = json.loads((directory / "snapshot.json").read_text())
    assert all(digest(directory / path) == sha for path, sha in m["files"].items()), "snapshot integrity mismatch"
    return m


def restore_fixture(root, baseline, retained, expected_sources):
    """Fixture-only matched rollback. Caller retains the complete upgraded runtime first."""
    m = verify_snapshot(baseline)
    assert m["root"] == str(root), "rollback is bound to the original project directory"
    # Refuse changed original sources before moving any runtime or source bytes.
    assert all(digest(root / p) == sha for p, sha in expected_sources.items()), "external source changes require review"
    (root / ".awr").rename(retained)
    shutil.copytree(baseline / "runtime", root / ".awr")
    for name in expected_sources:
        shutil.copy2(baseline / "sources" / name, root / name)
    # No deletion or broad directory replacement: new user/APP files remain in place.
    return baseline / m["program"]


def seed(program, root):
    root.mkdir()
    (root / "work.yaml").write_text("goals:\n- id: G\n  title: Reading\n  status: active\n  summary: Retain useful ideas.\nwork_items:\n- id: W\n  title: Read article\n  status: planned\n  goal: G\n  acceptance: [Useful note]\n  next_action: Read the article\n")
    (root / "GOAL.md").write_text("# Reading {#reading}\n\nRetain useful ideas.\n")
    (root / "mapping.toml").write_text("[project]\nname='Upgrade fixture'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='goal'\nrole='supporting'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n")
    (root / "legacy-app.json").write_text('{"W":{"completed":true,"old_session":"legacy-only"}}\n')
    (root / "legacy-app.json").chmod(0o444)
    native(program, root, "init", "--manifest", "mapping.toml", "--accept")
    rev = native(program, root, "session", "list")["project_revision"]
    session = native(program, root, "session", "start", "--work", "W", "--agent", "fixture", "--provider", "fixture", "--model", "fixture", "--claim", "--expected-revision", rev)
    sid = session["session"]["id"]
    context = native(program, root, "context", "compile", "--session", sid)
    saved = native(program, root, "session", "checkpoint", "--session", sid, "--agent", "fixture", "--work", "W", "--context-hash", context["work_context"]["context_hash"], "--digest", "Read the first part; retain this observation across upgrades.", "--next-action", "Continue the note", "--expected-revision", context["project_revision"])
    native(program, root, "session", "end", "--session", sid, "--expected-revision", saved["project_revision"])


def drill(old, new, out):
    out.mkdir(parents=True, exist_ok=False)
    root = out / "project-a"
    seed(old, root)
    legacy = digest(root / "legacy-app.json")
    before = db_rows(root)
    baseline = out / "baseline"
    old_manifest = snapshot(root, old, baseline)
    expected_sources = {p: digest(root / p) for p in ("work.yaml", "GOAL.md")}
    new_caps = native(new, root, "capabilities")
    assert new_caps["database"]["schema_version"] >= old_manifest["schema"]
    native(new, root, "source", "reindex")
    native(new, root, "doctor", "--database-only")
    after = db_rows(root)
    for table in ("sessions", "claims", "events", "checkpoints", "evidence", "artifacts"):
        assert set(before[table]).issubset(set(after[table])), f"lost runtime history: {table}"
    with sqlite3.connect(root / ".awr/state.db") as db:
        ids_after = {t: db.execute(f'SELECT id FROM "{t}" ORDER BY id').fetchall() for t in ("projects", "sources", "work_items")}
    with sqlite3.connect(baseline / "runtime/state.db") as db:
        ids_before = {t: db.execute(f'SELECT id FROM "{t}" ORDER BY id').fetchall() for t in ids_after}
    assert ids_before == ids_after, "upgrade changed project, source or task identity"
    assert digest(root / "legacy-app.json") == legacy
    assert native(new, root, "status")["organization"]["verified_completed"] == 0
    rejected = None
    if new_caps["database"]["schema_version"] > old_manifest["schema"]:
        protected = db_rows(root)
        rejected = native(old, root, "source", "reindex", success=False)
        assert db_rows(root) == protected, "old program changed the newer database"
        assert all(digest(root / p) == sha for p, sha in expected_sources.items())
    snapshot(root, new, out / "upgraded")
    (root / "user-added.md").write_text("New user work after upgrade.\n")
    added = digest(root / "user-added.md")
    original = (root / "work.yaml").read_bytes()
    (root / "work.yaml").write_bytes(original + b"\n# New external content\n")
    try:
        restore_fixture(root, baseline, out / "must-not-exist", expected_sources)
    except AssertionError as error:
        assert "external source changes" in str(error)
    else:
        raise AssertionError("rollback overwrote changed original source")
    assert not (out / "must-not-exist").exists()
    # Fixture explicitly resolves its own deliberate external edit; real users review theirs.
    (root / "work.yaml").write_bytes(original)
    restored_program = restore_fixture(root, baseline, out / "retained-upgraded-runtime", expected_sources)
    native(restored_program, root, "doctor", "--database-only")
    assert db_rows(root) == before, "matched rollback lost or promoted runtime records"
    assert digest(root / "user-added.md") == added
    assert digest(root / "legacy-app.json") == legacy
    other = out / "project-b"
    seed(new, other)
    a = native(restored_program, root, "work", "show", "W")
    b = native(new, other, "work", "show", "W")
    with sqlite3.connect(root / ".awr/state.db") as db_a, sqlite3.connect(other / ".awr/state.db") as db_b:
        assert db_a.execute("SELECT id FROM projects").fetchone() != db_b.execute("SELECT id FROM projects").fetchone()
    assert a["work"]["id"] != b["work"]["id"]
    assert digest(other / "legacy-app.json") == legacy
    report = dict(version=1, passed=True, old_program_sha256=digest(old), new_program_sha256=digest(new),
                  old_schema=old_manifest["schema"], new_schema=new_caps["database"]["schema_version"],
                  startup_environment="empty_environment_absolute_native_executable", table_counts_before={k: len(v) for k, v in before.items()},
                  project_source_work_ids_preserved=True, historical_rows_preserved=True,
                  old_program_rejection=rejected, rollback_matches_runtime_history=True,
                  changed_original_source_rollback_refused=True, new_user_files_preserved=True,
                  legacy_app_read_only_preserved=True, legacy_completion_not_promoted=True,
                  same_key_projects_isolated=True, dual_writers=False,
                  boundary="isolated native process upgrade/rollback fixtures; not application E4, signing or arbitrary directory relocation")
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--old-awr", required=True, type=Path)
    parser.add_argument("--new-awr", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(drill(args.old_awr.resolve(), args.new_awr.resolve(), args.output.resolve())))


if __name__ == "__main__":
    main()
