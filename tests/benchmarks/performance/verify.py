#!/usr/bin/env python3
"""Measure correct native CLI operations against fixed local p95 limits."""
import argparse
from contextlib import closing
from datetime import datetime
import json
import math
from pathlib import Path
import platform
import shutil
import sqlite3
import statistics
import subprocess
import sys
import time

import yaml

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'tests/long-ledger'))
from prepare import digest, source_state, write_json
from inspect_sample import inspect_bytes, require


def command(arguments):
    prefix = ['rtk', 'proxy'] if shutil.which('rtk') else []
    return subprocess.run(prefix + list(map(str, arguments)), cwd=ROOT, capture_output=True,
                          text=True, encoding='utf-8', timeout=60)


def checked(*arguments):
    result = command(arguments)
    require(result.returncode == 0, 'Environment/source command failed: ' + arguments[0])
    return result.stdout.strip()


def statistics_for(samples, expected):
    require(len(samples) == expected >= 30, 'Incomplete latency sample set')
    require(all(s['correct'] and math.isfinite(s['elapsed_ms']) and s['elapsed_ms'] >= 0 for s in samples),
            'Failed or invalid samples cannot be excluded from latency results')
    values = sorted(s['elapsed_ms'] for s in samples)
    return {'samples': len(values), 'minimum_ms': values[0], 'median_ms': statistics.median(values),
            'p95_ms': values[math.ceil(len(values) * 0.95) - 1], 'maximum_ms': values[-1]}


def counts(project):
    with closing(sqlite3.connect((project / '.awr/state.db').as_uri() + '?mode=ro', uri=True)) as db:
        db.execute('BEGIN')
        return {table: db.execute('SELECT COUNT(*) FROM ' + table).fetchone()[0]
                for table in ['work_items', 'goals', 'plans', 'rules', 'decisions', 'evidence', 'edges', 'sources',
                              'events', 'sessions', 'claims', 'checkpoints', 'artifacts']}


def work_revisions(project):
    with closing(sqlite3.connect((project / '.awr/state.db').as_uri() + '?mode=ro', uri=True)) as db:
        return dict(db.execute('SELECT external_key, revision FROM work_items'))


def source_hashes(project):
    return {p.relative_to(project).as_posix(): digest(p) for p in sorted(project.rglob('*'))
            if p.is_file() and '.awr' not in p.relative_to(project).parts and p.name != '.gitignore'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--awr', type=Path, required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary, prepared = args.awr.resolve(strict=True), args.prepared.resolve(strict=True)
    output = args.output.resolve()
    for path in [prepared, output]:
        path.relative_to(ROOT / '.local')
    require(binary == (ROOT / 'target/release/awr').resolve(), 'Use the locked release binary')
    require(not output.exists(), 'Use a new output directory; preserve failed runs')
    require(not checked('git', 'status', '--porcelain', '--untracked-files=all'), 'Commit all benchmark inputs first')
    contract = json.loads((HERE / 'contract.json').read_text())
    sample_contract = json.loads((ROOT / contract['sample_contract']).read_text())
    preparation = json.loads((prepared / 'preparation.json').read_text())
    for name, key in [('preparation.json', 'preparation_sha256'), ('mapping-spec.json', 'mapping_sha256'), ('source-map.json', 'source_map_sha256')]:
        require(digest(prepared / name) == sample_contract['sample'][key], 'Frozen real-source binding changed')
    require(source_hashes(prepared / 'project') == preparation['project_files'], 'Frozen source files changed')
    intake = Path(preparation['intake_path']).resolve(strict=True)
    original = source_state(intake)
    spec = json.loads((prepared / 'mapping-spec.json').read_text())
    document = yaml.safe_load((prepared / 'project/work-ledger.yaml').read_text())
    selected = next(w for w in document['work_items'] if w['id'] == spec['selected_work'])
    admission = inspect_bytes((prepared / 'project/work-ledger.yaml').read_bytes(),
        json.loads((ROOT / sample_contract['sample']['admission_contract']).read_text()))
    require(admission['scale_eligible'], 'Real sample scale changed')
    environment = {'system': platform.system(), 'machine': platform.machine(), 'os_version': platform.platform(),
                   'python': platform.python_version(), 'logical_cpus': __import__('os').cpu_count(),
                   'rustc': checked('rustc', '-Vv'), 'cargo': checked('cargo', '-V'),
                   'rtk': checked('rtk', '--version') if shutil.which('rtk') else None}
    if platform.system() == 'Darwin':
        environment.update(cpu_model=checked('sysctl', '-n', 'machdep.cpu.brand_string'),
                           memory_bytes=int(checked('sysctl', '-n', 'hw.memsize')),
                           os_build=checked('sw_vers'))
    else:
        require(False, 'This frozen host protocol requires macOS hardware reporting; extend it explicitly for another host')
    output.mkdir(parents=True)
    report = {'kind': 'local_cli_latency_benchmark', 'contract_id': contract['contract_id'], 'version': contract['version'],
              'source_commit': checked('git', 'rev-parse', 'HEAD'), 'binary_sha256': digest(binary),
              'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'), 'environment': environment,
              'sample': sample_contract['sample'], 'build': contract['build'], 'transport': contract['transport'],
              'cache': contract['cache'], 'percentile': contract['percentile'], 'passed': False, 'operations': [],
              'inputs': {p.relative_to(ROOT).as_posix(): digest(p) for p in sorted(HERE.iterdir()) if p.is_file()},
              'e4_completed': 0, 'model_invocations': 0}
    report['inputs'][contract['sample_contract']] = digest(ROOT / contract['sample_contract'])

    def save():
        write_json(output / 'report.json', report)

    save()
    try:
        for operation in contract['operations']:
            metric = operation['metric']
            base = output / metric
            project = base / 'project'
            receipts = base / 'receipts'
            receipts.mkdir(parents=True)
            shutil.copytree(prepared / 'project', project)
            row = {'metric': metric, 'exclusive_limit_ms': operation['exclusive_limit_ms'], 'passed': False,
                   'commands': [], 'warmups': [], 'samples': []}
            report['operations'].append(row)
            save()

            def run(label, arguments):
                start = time.perf_counter_ns()
                result = command([binary, '--project', project, '--json', *arguments])
                elapsed = (time.perf_counter_ns() - start) / 1_000_000
                receipt = receipts / (label + '.json')
                require(not receipt.exists(), 'Duplicate command label')
                write_json(receipt, {'arguments': list(map(str, arguments)), 'elapsed_ms': elapsed,
                                    'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
                record = {'label': label, 'elapsed_ms': elapsed, 'correct': False, 'exit_code': result.returncode,
                          'receipt': receipt.relative_to(output).as_posix(), 'sha256': digest(receipt),
                          'stdout_utf8_bytes': len(result.stdout.encode('utf-8'))}
                row['commands'].append(record)
                save()
                require(result.returncode == 0, 'Native command failed: ' + metric + '/' + label)
                return json.loads(result.stdout), record

            init, record = run('init', ['init', '--manifest', 'project.toml', '--accept'])
            require(init['index']['ok'], 'Sample did not index')
            record['correct'] = True
            status, record = run('baseline-status', ['status'])
            require(status['ok'] and status['total'] == len(document['work_items']), 'Baseline status changed')
            record['correct'] = True
            revision = status['project_revision']
            work, record = run('baseline-work', ['object', 'show', 'work', selected['id'], '--full'])
            work = work['object']
            require(work['external_key'] == selected['id'] and work['next_action'] == selected['next_action'], 'Baseline work changed')
            record['correct'] = True
            row.update(initial_counts=counts(project), ready_count=status['ready_count'],
                       ledger_bytes=(project / 'work-ledger.yaml').stat().st_size,
                       authority_bytes=sum((project / name).stat().st_size for name in preparation['project_files']))
            before_revisions = work_revisions(project)
            goals = [v for g in selected['goals'] for v in ['--goal', g]]
            arguments = {
                'status_latency': ['status'], 'work_show_latency': ['work', 'show', selected['id']],
                'ready_latency': ['ready', '--limit', '20'],
                'context_compile_latency': ['context', 'compile', '--work', selected['id'], *goals, '--after-revision', revision, '--budget', 5000],
                'fts_latency': ['search', selected['id'], '--type', 'work', '--limit', 10],
                'incremental_reindex_latency': ['source', 'reindex']}[metric]
            current = None
            phases = ['first-observed'] + ['warmup-' + str(i + 1) for i in range(contract['warmups'])] + [
                'sample-' + str(i + 1) for i in range(contract['samples'])]
            for index, label in enumerate(phases):
                if metric == 'incremental_reindex_latency':
                    overlay = yaml.safe_load((prepared / 'project/work-ledger.yaml').read_text())
                    changed = next(w for w in overlay['work_items'] if w['id'] == selected['id'])
                    current = selected['next_action'] + '\nBenchmark source revision ' + str(index + 1) + '.'
                    changed['next_action'] = current
                    (project / 'work-ledger.yaml').write_text(yaml.safe_dump(overlay, allow_unicode=True, sort_keys=False, width=120))
                body, record = run(label, arguments)
                require(body.get('ok', True), 'Command returned a failed/incomplete result')
                if metric == 'status_latency':
                    require(body['project_id'] == status['project_id'] and body['total'] == status['total'] and
                            body['ready_count'] == status['ready_count'], 'Status result lost current facts')
                elif metric == 'work_show_latency':
                    require(body['work']['id'] == work['id'] and body['work']['next_action'] == selected['next_action'] and
                            body['acceptance'] == selected['acceptance'], 'Work read lost selected source facts')
                elif metric == 'ready_latency':
                    require(body['ready_total'] == status['ready_count'] and len(body['ready']) <= 20 and
                            all(w['ready'] for w in body['ready']) and not any(w['external_key'] == selected['id'] for w in body['ready']),
                            'Ready query disagrees with known work state')
                elif metric == 'context_compile_latency':
                    pack = body['work_context']
                    require(body['completeness']['complete'] and pack['token_estimate'] <= 5000 and
                            pack['identity']['work_item_id'] == work['id'] and all(a in pack['rendered_context'] for a in selected['acceptance']),
                            'Context compilation lost required task facts')
                elif metric == 'fts_latency':
                    require(any(hit['external_key'] == selected['id'] and hit['kind'] == 'work_item' for hit in body['hits']),
                            'FTS did not retrieve the actual selected work')
                else:
                    updated, verification = run('verify-' + label, ['object', 'show', 'work', selected['id'], '--full'])
                    updated = updated['object']
                    require(updated['id'] == work['id'] and updated['revision'] == work['revision'] + index + 1
                            and updated['next_action'] == current, 'Reindex was a no-op or changed the wrong work')
                    verification['correct'] = True
                record['correct'] = True
                if label == 'first-observed':
                    row['first_observed_ms'] = record['elapsed_ms']
                elif label.startswith('warmup-'):
                    row['warmups'].append(record)
                else:
                    row['samples'].append(record)
                save()
            row['statistics'] = statistics_for(row['samples'], contract['samples'])
            row['passed'] = row['statistics']['p95_ms'] < operation['exclusive_limit_ms']
            row['final_counts'] = counts(project)
            after_revisions = work_revisions(project)
            if metric == 'incremental_reindex_latency':
                expected = dict(before_revisions)
                expected[selected['id']] += len(phases)
                require(after_revisions == expected, 'Incremental changes modified unrelated task revisions')
                observed = yaml.safe_load((project / 'work-ledger.yaml').read_text())
                last = next(w for w in observed['work_items'] if w['id'] == selected['id'])
                last['next_action'] = selected['next_action']
                require(observed == document, 'The performance overlay changed unrelated source facts')
                row['verified_source_changes'] = len(phases)
            else:
                require(after_revisions == before_revisions and source_hashes(project) == preparation['project_files'],
                        'Read benchmark changed source facts')
            save()
        require(source_state(intake) == original and source_hashes(prepared / 'project') == preparation['project_files'],
                'Original or frozen sample changed during execution')
        require(all(digest(ROOT / name) == value for name, value in report['inputs'].items()) and digest(binary) == report['binary_sha256'],
                'Benchmark input or binary changed during execution')
        report['metrics'] = {row['metric']: row['statistics']['p95_ms'] for row in report['operations']}
        report['passed'] = all(row['passed'] for row in report['operations'])
        report['gates'] = {name: True for name in contract['required_gates']}
        report['gates']['six_p95_thresholds'] = report['passed']
        save()
        summary = {k: report[k] for k in report if k != 'operations'}
        summary['operations'] = [{k: row[k] for k in ['metric', 'exclusive_limit_ms', 'passed', 'first_observed_ms',
            'statistics', 'initial_counts', 'final_counts', 'ready_count', 'ledger_bytes', 'authority_bytes']} for row in report['operations']]
        for public, row in zip(summary['operations'], report['operations']):
            public.update(samples_ms=[s['elapsed_ms'] for s in row['samples']], warmups_ms=[s['elapsed_ms'] for s in row['warmups']],
                          calls=len(row['commands']), verified_source_changes=row.get('verified_source_changes', 0),
                          setup_calls_ms={c['label']: c['elapsed_ms'] for c in row['commands'][:3]})
        summary.update(private_report_sha256=digest(output / 'report.json'), limitations=[contract['boundary'], contract['cache']])
        write_json(output / 'public-summary.json', summary)
        print(json.dumps({'passed': report['passed'], 'metrics': report['metrics'],
                          'calls': sum(len(row['commands']) for row in report['operations'])}, indent=2))
        return 0 if report['passed'] else 1
    except Exception as error:
        report['failure'] = {'type': type(error).__name__, 'message': str(error)}
        save()
        raise


if __name__ == '__main__':
    sys.exit(main())
