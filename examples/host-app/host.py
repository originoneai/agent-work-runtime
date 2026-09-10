"""Small no-UI host example. Only argv/JSON talks to AWR; no private DB access.

Python is a developer convenience for this example, not an AWR runtime dependency.
"""
import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def protected_file(directory, suffix, body):
    fd, name = tempfile.mkstemp(dir=directory, suffix=suffix)
    with os.fdopen(fd, "wb") as stream:
        stream.write(body)
    return Path(name)


def decode(body):
    try:
        return json.loads(body)
    except (ValueError, UnicodeDecodeError):
        return None


@dataclass
class Result:
    exit_code: int | None
    stdout: bytes
    stderr: bytes
    receipt: Path
    outcome_unknown: bool = False

    @property
    def value(self):
        return decode(self.stdout)

    @property
    def error(self):
        return decode(self.stderr)

    def require(self):
        if self.outcome_unknown or self.exit_code != 0 or not isinstance(self.value, dict):
            raise CommandFailed(self)
        return self.value


class CommandFailed(RuntimeError):
    def __init__(self, result):
        self.result = result
        super().__init__(f"AWR command incomplete; inspect {result.receipt}")


class Host:
    def __init__(self, binary, expected_sha256, project, receipts, timeout=60):
        if not Path(binary).is_absolute():
            raise ValueError("Use an absolute, pinned executable path")
        self.binary = Path(binary).resolve(strict=True)
        self.expected_sha256 = expected_sha256
        self.verify_binary()
        self.project = Path(project).resolve(strict=True)
        self.receipts = Path(receipts).resolve()
        # The caller supplies a NEW private directory, outside registered sources.
        self.receipts.mkdir(mode=0o700, parents=True, exist_ok=False)
        self.timeout = timeout

    def verify_binary(self):
        if digest(self.binary) != self.expected_sha256:
            raise ValueError("Executable differs from the trusted pinned checksum")

    def call(self, *args, stdin=None):
        self.verify_binary()
        argv = [str(self.binary), "--project", str(self.project), "--json", *args]
        request = dict(argv=argv, started_at_ms=time.time_ns() // 1_000_000,
                       binary_sha256=self.expected_sha256, outcome="pending")
        receipt = protected_file(self.receipts, ".json", json.dumps(request).encode())
        # A pending receipt after a host crash is also an UNKNOWN outcome.
        try:
            process = subprocess.run(argv, input=None if stdin is None else
                                     json.dumps(stdin, ensure_ascii=False).encode(),
                                     stdin=subprocess.DEVNULL if stdin is None else None,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     timeout=self.timeout, shell=False)
            result = Result(process.returncode, process.stdout, process.stderr, receipt)
        except subprocess.TimeoutExpired as error:
            result = Result(None, error.stdout or b"", error.stderr or b"", receipt, True)
        except OSError as error:
            result = Result(None, b"", str(error).encode(), receipt, True)
        out = protected_file(self.receipts, ".stdout", result.stdout)
        err = protected_file(self.receipts, ".stderr", result.stderr)
        request.update(exit_code=result.exit_code, stdout=out.name, stderr=err.name,
                       finished_at_ms=time.time_ns() // 1_000_000,
                       outcome="unknown" if result.outcome_unknown else "returned")
        receipt.write_text(json.dumps(request, ensure_ascii=False, indent=2), encoding="utf-8")
        return result

    def ok(self, *args, **kwargs):
        return self.call(*args, **kwargs).require()

    def input(self, args, value):
        path = protected_file(self.receipts, ".input.json",
                              json.dumps(value, ensure_ascii=False).encode())
        return self.call(*args, "--input", str(path))

    def discover(self, expected_version, requirements):
        args = ["capabilities", "--protocol-version", "1"]
        for name in requirements:
            args += ["--require", name]
        value = self.ok(*args)
        if value["program"]["version"] != expected_version:
            raise ValueError("Unexpected program version")
        return value

    def catalog(self, kind, scope="active", limit=20):
        items, pages, cursor, revision, total = [], 0, None, None, None
        while True:
            args = ["object", "list", kind, "--scope", scope, "--limit", str(limit)]
            if cursor is not None:
                args += ["--cursor", json.dumps(cursor)]
            page = self.ok(*args)  # A partial stdout result never completes traversal.
            if not page["total_is_current"]:
                raise ValueError("Source catalog is partial")
            if pages and (revision, total) != (page["project_revision"], page["total"]):
                raise ValueError("Mixed catalog versions; discard and restart explicitly")
            revision, total = page["project_revision"], page["total"]
            items += page["items"]
            pages += 1
            if not page["has_more"]:
                break
            cursor = page["next_cursor"]
            if cursor is None:
                raise ValueError("Missing next cursor")
        if len(items) != total or len({v["id"] for v in items}) != total:
            raise ValueError("Catalog traversal has missing or duplicate identities")
        return dict(items=items, total=total, pages=pages, project_revision=revision)


def demonstration(host):
    """Synthetic driver, explicit lifecycle callbacks, no model/client launched."""
    before = (host.project / "工作 台账.yaml").read_bytes()
    preview = host.ok("init", "--manifest", "mapping.toml")
    if (host.project / ".awr").exists():
        raise ValueError("Demo expects an uninitialized fixture")
    host.ok("init", "--manifest", "mapping.toml", "--accept", "--expected-preview",
            preview["preview"]["fingerprint"])
    catalog = host.catalog("work")
    first = "synthetic-host/conversation-one"
    bound = host.ok("client", "bind", "--client", "generic", "--external-session",
                    first, "--work", "GUIDE-01", "--model", "synthetic-no-model")
    session = bound["binding"]["session_id"]
    context = host.ok("context", "compile", "--session", session)
    if not context["completeness"]["complete"]:
        raise ValueError("Do not deliver incomplete context")
    execution = host.ok("execution", "register", "--session", session, "--key",
                        "synthetic-outline", "--purpose", "Demonstrate host continuity",
                        "--reference", "host://synthetic/outline")
    revision = host.ok("session", "list")["project_revision"]
    report = dict(version=1, request_key="synthetic-outline/progress",
                  execution_id=execution["execution"]["id"], host_id="synthetic-host",
                  host_work_key="guide-outline", native_session=first,
                  agent_id="synthetic-driver", origin="caller_reported", phase="progress",
                  observed_at=time.time_ns() // 1_000_000,
                  summary="The fixture driver reached the outline stage", detail_references=[])
    recorded = host.input(["execution", "report", "--expected-revision", str(revision)], report).require()
    lookup = host.ok("execution", "report-status", "--key", report["request_key"])
    assert lookup["event"]["id"] == recorded["event"]["id"]
    host.ok("client", "progress", "--client", "generic", "--external-session", first,
            "--digest", "Outline recorded by the synthetic driver",
            "--next-action", "Review the guide examples", "--open-loop", "Review remains pending")
    hook = dict(session_id=first, cwd=str(host.project), hook_event_name="Stop", turn_id="outline")
    saved = host.ok("client", "hook", "--client", "generic", "--work", "GUIDE-01", stdin=hook)
    replay = host.ok("client", "hook", "--client", "generic", "--work", "GUIDE-01", stdin=hook)
    assert saved["awr"]["checkpoint_saved"] and replay["awr"]["duplicate"]
    checkpoint = saved["awr"]["binding"]["checkpoint_id"]
    host.ok("object", "show", "checkpoint", checkpoint, "--full")
    inspection = host.ok("recovery", "inspect", "--session", session)
    assert not inspection["side_effects_performed"]
    assert inspection["checkpoint"]["id"] == checkpoint
    # Preserve doctor diagnostics independently; findings can be nonzero.
    doctor = host.call("doctor")
    assert doctor.value["read_only"]
    resumed = host.ok("client", "bind", "--client", "generic", "--external-session",
                      "synthetic-host/conversation-two", "--work", "GUIDE-01", "--from-session",
                      session, "--model", "synthetic-no-model")
    assert resumed["binding"]["session_id"] != session
    assert resumed["binding"]["next_action"] == "Review the guide examples"
    assert (host.project / "工作 台账.yaml").read_bytes() == before
    # Explicitly end both fixture sessions, without claiming the guide is completed.
    for sid in (session, resumed["binding"]["session_id"]):
        state = host.ok("session", "show", sid)
        if state["session"]["status"] == "active":
            revision = host.ok("session", "list")["project_revision"]
            host.ok("session", "end", "--session", sid, "--outcome", "ended",
                    "--expected-revision", str(revision))
    return dict(boundary="synthetic host; no native model client or business acceptance",
                work_total=catalog["total"], pages=catalog["pages"], checkpoint=checkpoint,
                first_session=session, successor_session=resumed["binding"]["session_id"],
                context_hash=context["work_context"]["context_hash"],
                source_bytes_unchanged=True, doctor_exit_code=doctor.exit_code)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--sha256", required=True, help="Checksum from a trusted build/release receipt")
    parser.add_argument("--version", required=True)
    sub = parser.add_subparsers(dest="mode", required=True)
    demo = sub.add_parser("demo", help="Create a NEW synthetic fixture directory")
    demo.add_argument("--directory", required=True)
    inspect = sub.add_parser("inspect", help="Read complete work catalog; may refresh runtime only")
    inspect.add_argument("--project", required=True)
    inspect.add_argument("--receipts", required=True, help="NEW private output directory")
    args = parser.parse_args()
    if args.mode == "demo":
        project = Path(args.directory).resolve()
        fixture = Path(__file__).resolve().parents[2] / "tests/fixtures/host-app"
        shutil.copytree(fixture, project)  # Fails if destination exists; never clobbers a project.
        receipts = project / ".local/host-receipts"
    else:
        project, receipts = args.project, args.receipts
    host = Host(args.binary, args.sha256, project, receipts)
    host.discover(args.version, ["project.catalog", "intake.exact_preview",
                                "client.lifecycle.generic", "execution.external.report"])
    result = demonstration(host) if args.mode == "demo" else host.catalog("work")
    if args.mode == "inspect":
        result = {k: v for k, v in result.items() if k != "items"}
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
