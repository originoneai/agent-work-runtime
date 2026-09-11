#!/usr/bin/env python3
"""Exercise native paths, persistent state and reviewed writes on a new copied project."""
import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--awr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    output, report_path = args.output.resolve(), args.report.resolve()
    output.relative_to(ROOT / '.local')
    report_path.relative_to(ROOT / '.local')
    require(not report_path.exists(), 'Use a new report path')
    output.mkdir(parents=True, exist_ok=False)
    report_path.parent.mkdir(parents=True, exist_ok=True)
    project = output / '项目 with spaces'
    project.mkdir()
    binary = args.awr.resolve(strict=True)
    report = {'kind': 'native_cli_fixture', 'passed': False, 'conditions': {}, 'commands': [],
              'project_root': str(project), 'binary_sha256': digest(binary), 'e4_completed': 0}

    def save():
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

    def run(label, *arguments, accepted=True, root=project):
        command = [str(binary), '--project', str(root), '--json', *map(str, arguments)]
        result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', timeout=60)
        receipt = report_path.parent / (label + '.json')
        receipt.write_text(json.dumps({'command': command, 'exit_code': result.returncode,
                                      'stdout': result.stdout, 'stderr': result.stderr},
                                     ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        report['commands'].append({'label': label, 'exit_code': result.returncode,
                                   'receipt': str(receipt.relative_to(ROOT)), 'sha256': digest(receipt)})
        save()
        require((result.returncode == 0) == accepted, label + ' returned an unexpected exit code')
        payload = json.loads(result.stdout if result.returncode == 0 else result.stderr)
        if result.returncode != 0 and result.stdout:
            payload['stdout_report'] = json.loads(result.stdout)
        return payload

    for name in ['project.toml', 'GOALS.md', 'PLAN.md', 'RULES.md', 'work-ledger.yaml']:
        source = ROOT / 'examples' / ('codex' if name == 'work-ledger.yaml' else 'basic') / name
        shutil.copyfile(source, project / name)
    ledger = project / 'work-ledger.yaml'
    ledger.write_bytes(ledger.read_bytes().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n'))
    initial = run('01-init', 'init', '--manifest', 'project.toml', '--accept')
    require(initial['index']['ok'], 'Native project source intake failed')
    work = run('02-work', 'work', 'show', 'EXAMPLE-001')
    require(work['work']['ready'], 'CRLF source lost the ready work')
    report['conditions'].update(unicode_space_path=True, crlf_source_intake=True)
    db_path = project / '.awr/state.db'

    def state():
        with closing(sqlite3.connect(db_path.as_uri() + '?mode=ro', uri=True)) as db:
            db.execute('BEGIN')
            schema = db.execute('SELECT type,name,sql FROM sqlite_master ORDER BY type,name').fetchall()
            tables = {}
            for kind, name, _ in schema:
                if kind == 'table':
                    query = 'SELECT * FROM "' + name.replace('"', '""') + '"'
                    tables[name] = sorted(json.dumps(row, default=lambda b: {'blob': b.hex()}) for row in db.execute(query))
            material = json.dumps({'schema': schema, 'tables': tables}, sort_keys=True).encode()
            return {'database_sha256': hashlib.sha256(material).hexdigest(), 'source_sha256': digest(ledger),
                    'revision': db.execute('SELECT project_revision FROM projects').fetchone()[0]}

    with closing(sqlite3.connect(db_path.as_uri() + '?mode=ro', uri=True)) as db:
        require(db.execute('PRAGMA journal_mode').fetchone()[0] == 'wal', 'WAL is not active')
        require(db.execute('PRAGMA user_version').fetchone()[0] == 4, 'Unexpected schema version')
        require(db.execute('PRAGMA integrity_check').fetchall() == [('ok',)], 'Database integrity failed')
        require(not db.execute('PRAGMA foreign_key_check').fetchall(), 'Foreign keys failed')
    report['conditions']['wal_schema_integrity'] = True
    started = run('03-start', 'session', 'start', '--work', 'EXAMPLE-001', '--agent', 'platform-fixture',
                  '--provider', 'fixture', '--model', 'no-model-invocation', '--claim', '--ttl-ms', 3600000,
                  '--expected-revision', work['project_revision'])
    sid = started['session']['id']
    shown = run('04-session-reopen', 'session', 'show', sid)
    require(shown['session'] == started['session'] and shown['claims'][0]['id'] == started['claim']['id'],
            'A new process did not retain the session and claim')
    report['conditions']['persisted_session'] = True
    before = ledger.read_bytes()
    next_action = 'Review the portable source write.'
    applied = run('05-progress', 'work', 'progress', 'EXAMPLE-001', '--session', sid,
                  '--reason', 'Record the native CLI persistence check.', '--next-action', next_action,
                  '--expected-revision', started['project_revision'])
    recovery = project / applied['recovery_directory']
    require(applied['proposal']['status'] == 'applied', 'Source action did not apply')
    require(applied['apply_attempt']['resolved_event_id'] == applied['event']['id'], 'Write receipt lost the attempt')
    require((recovery / 'before.yaml').read_bytes() == before and
            (recovery / 'after.yaml').read_bytes() == ledger.read_bytes(), 'Write recovery snapshots mismatch')
    reopened = run('06-work-reopen', 'object', 'show', 'work', 'EXAMPLE-001', '--full', '--cached')
    require(reopened['object']['next_action'] == next_action, 'A new process lost the source projection')
    report['conditions']['source_write_and_receipt'] = True
    stable = state()
    error = run('07-stale-write', 'work', 'progress', 'EXAMPLE-001', '--session', sid,
                '--reason', 'Try an obsolete source revision.', '--next-action', 'Obsolete action must not apply.',
                '--expected-revision', started['project_revision'], accepted=False)
    require(error['code'] == 'RevisionConflict' and state() == stable, 'Stale write changed persisted state')
    report['conditions']['stale_write_preserved'] = True
    rules = project / 'RULES.md'
    rules.write_bytes(rules.read_bytes() + b'\r\n# Native handoff {#native-handoff severity=hard scope=project value=*}\r\n\r\nHold the portable handoff note.\r\n')
    context = run('08-updated-context', 'context', 'compile', '--session', sid, '--budget', 5000)
    require(context['completeness']['complete'] and
            'Hold the portable handoff note.' in context['work_context']['rendered_context'], 'Updated hard rule missing')
    report['conditions']['updated_hard_rule'] = True
    outside = output / 'outside.yaml'
    outside.write_text('work_items: []\n', encoding='utf-8')
    outside_hash = digest(outside)
    for label, locator in [('traversal', '../outside.yaml'), ('absolute', outside.as_posix())]:
        denied = output / label
        denied.mkdir()
        (denied / 'project.toml').write_text("[project]\nname='Denied source'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='" + locator + "'\nadapter='yaml-ledger-v1'\n", encoding='utf-8')
        rejected = run('09-denied-' + label, 'init', '--manifest', 'project.toml', '--accept', accepted=False, root=denied)
        details = rejected['details']
        require(rejected['code'] == 'SourceStale' and details['can_apply'] is False
                and details['source_issues'], 'Outside source did not report its rejection')
        require(all(details[field] is False for field in
                    ['configuration_write_performed', 'runtime_write_performed',
                     'source_write_performed', 'intake_staged']),
                'Rejected preflight reported a write')
        # Preflight rejects invalid sources before creating configuration or a database.
        require(not (denied / '.awr').exists(), 'Denied outside source created runtime state')
        require(digest(outside) == outside_hash, 'Denied outside source was changed')
    report['conditions']['outside_source_rejected'] = True
    status = run('10-status', 'status')
    ended = run('11-end', 'session', 'end', '--session', sid, '--outcome', 'incomplete',
                '--expected-revision', status['project_revision'])
    stable = state()
    doctor = run('12-doctor', 'doctor')
    require(doctor['ok'] and not doctor['findings'] and state() == stable, 'Native Doctor changed state or found problems')
    shown = run('13-ended-reopen', 'session', 'show', sid)
    require(shown['session']['status'] == 'incomplete' and not any(c['status'] == 'active' for c in shown['claims']),
            'Native session or claim remained active')
    report['conditions']['closed_session_health'] = True
    expected = json.loads((ROOT / 'tests/platform/contract.json').read_text())['native_cli_conditions']
    require(set(report['conditions']) == set(expected) and all(report['conditions'].values()), 'Native condition coverage is incomplete')
    report.update(passed=True, context_tokens=context['work_context']['token_estimate'], session_id=sid,
                  final_project_revision=ended['project_revision'], final_state=stable)
    save()
    print(json.dumps({'passed': True, 'conditions': len(expected), 'e4_completed': 0}))


if __name__ == '__main__':
    main()
