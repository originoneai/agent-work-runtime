#!/usr/bin/env python3
"""Run the real-ledger contract on new disposable copies; retain private receipts."""
import argparse
from contextlib import closing
from datetime import datetime
import json
from pathlib import Path
import platform
import re
import shutil
import sqlite3
import subprocess

import yaml

from prepare import ROOT, HERE, digest, digest_bytes, git, require, source_state, write_json

DOMAIN_TABLES = ('goals', 'plans', 'rules', 'work_items', 'decisions')


def source_files(project):
    return {path.relative_to(project).as_posix(): digest(path) for path in sorted(project.rglob('*'))
            if path.is_file() and '.awr' not in path.relative_to(project).parts and path.name != '.gitignore'}


def snapshot(project):
    """Read-only domain snapshot, including identities/revisions and runtime counts."""
    with closing(sqlite3.connect((project / '.awr/state.db').as_uri() + '?mode=ro', uri=True)) as db:
        db.row_factory = sqlite3.Row
        db.execute('BEGIN')
        result = {table: [dict(row) for row in db.execute('SELECT * FROM ' + table + ' ORDER BY id')]
                  for table in (*DOMAIN_TABLES, 'edges', 'evidence', 'sources')}
        result['runtime_counts'] = {table: db.execute('SELECT COUNT(*) FROM ' + table).fetchone()[0]
                                    for table in ('sessions', 'claims', 'checkpoints', 'artifacts')}
        result['events'] = [dict(row) for row in db.execute('SELECT * FROM events ORDER BY id')]
        result['project'] = dict(db.execute('SELECT * FROM projects').fetchone())
        return result


def facts(state, project):
    """Compare source-derived business facts across independent project identities."""
    def local(value):
        if isinstance(value, str):
            return value.replace(project.as_uri(), 'file:<project>').replace(str(project), '<project>')
        if isinstance(value, list):
            return [local(v) for v in value]
        if isinstance(value, dict):
            return {k: local(v) for k, v in value.items()}
        return value

    result = {}
    for table in DOMAIN_TABLES:
        values = {}
        for row in state[table]:
            data = json.loads(row['payload_json'])
            for key in ('id', 'project_id', 'source_ref', 'revision', 'active'):
                data.pop(key, None)
            values[local(row['external_key'])] = local(data)
        result[table] = values
    result['edges'] = sorted((row['from_kind'], local(row['from_key']), row['relation'], row['to_kind'],
                              local(row['to_key']), row['required'], row['active']) for row in state['edges'])
    work_keys = {row['id']: row['external_key'] for row in state['work_items']}
    result['evidence'] = sorted((local(row['external_key']), work_keys.get(row['work_item_id']),
                                 row['evidence_type'], row['level'], row['summary'], row['locator'])
                                for row in state['evidence'])
    return result


def identities(state):
    return {table: {row['id']: row['revision'] for row in state[table]}
            for table in (*DOMAIN_TABLES, 'edges', 'evidence', 'sources')}


def run_benchmark(binary, prepared, output):
    require(not git(ROOT, 'status', '--porcelain', '--untracked-files=all'), 'Commit benchmark inputs before execution')
    preparation = json.loads((prepared / 'preparation.json').read_text(encoding='utf-8'))
    spec = json.loads((prepared / 'mapping-spec.json').read_text(encoding='utf-8'))
    contract = json.loads((HERE / 'contract.json').read_text(encoding='utf-8'))
    intake = Path(preparation['intake_path']).resolve(strict=True)
    require(source_state(intake) == preparation['source_state'], 'Real source changed since preparation')
    require(source_files(prepared / 'project') == preparation['project_files'], 'Prepared files changed')
    require(digest(prepared / 'mapping-spec.json') == preparation['mapping_sha256'] and
            digest(prepared / 'source-map.json') == preparation['source_map_sha256'], 'Mapping changed')
    require(preparation['preparation_code'] == {name: digest(HERE / name) for name in preparation['preparation_code']},
            'Preparation code changed; regenerate a new prepared copy')
    output.mkdir(parents=True)
    receipts = output / 'receipts'
    receipts.mkdir()
    project = output / 'project'
    shutil.copytree(prepared / 'project', project)
    report = {'kind': 'real_large_ledger_benchmark', 'contract_id': contract['contract_id'],
              'contract_version': contract['version'], 'source_commit': git(ROOT, 'rev-parse', 'HEAD'),
              'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'),
              'environment': {'system': platform.system(), 'machine': platform.machine(), 'python': platform.python_version()},
              'binary_sha256': digest(binary), 'preparation_sha256': digest(prepared / 'preparation.json'),
              'mapping_sha256': preparation['mapping_sha256'], 'source_map_sha256': preparation['source_map_sha256'],
              'inputs': {path.name: digest(path) for path in sorted(HERE.glob('*')) if path.is_file()},
              'original_source_commit': preparation['source_state']['source_commit'],
              'commands': [], 'gates': {}, 'passed': False, 'e4_credit': 0, 'metric_credit': 0,
              'model_invocations': 0, 'original_backend_commands_executed': 0,
              'retention': 'local_only', 'mapping_gaps': spec['known_mapping_gaps']}

    def save():
        write_json(output / 'report.json', report)

    def gate(name, condition, message):
        report['gates'][name] = bool(condition)
        save()
        require(condition, message)

    def run(label, *arguments, target=project):
        prefix = ['rtk', 'proxy'] if shutil.which('rtk') else []
        result = subprocess.run(prefix + [str(binary), '--project', str(target), '--json', *map(str, arguments)],
                                cwd=ROOT, capture_output=True, text=True, encoding='utf-8', timeout=60)
        receipt = receipts / (label + '.json')
        require(not receipt.exists(), 'Command receipt already exists')
        write_json(receipt, {'arguments': list(map(str, arguments)), 'project': str(target),
                             'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
        report['commands'].append({'label': label, 'exit_code': result.returncode, 'sha256': digest(receipt)})
        save()
        require(result.returncode == 0, 'Native command failed; inspect the retained private receipt: ' + label)
        return json.loads(result.stdout)

    save()
    try:
        document = yaml.safe_load((project / 'work-ledger.yaml').read_text(encoding='utf-8'))
        work = {item['id']: item for item in document['work_items']}
        selected, historical = work[spec['selected_work']], work[spec['unrelated_historical_work']]
        gate('real_project_provenance', bool(preparation['source_state']['source_worktree_clean']), 'Missing real Git provenance')
        test_references = sorted(set(re.findall(r'`([^`\n ]+-(?:check|test))`', '\n'.join(item['summary'] for item in work.values()))))
        report['scale'] = {key: preparation['admission'][key] for key in
                           ('source_bytes', 'unique_work_items', 'current_work_items', 'historical_work_items', 'unknown_status_items')}
        report['richness'] = {key: preparation[key] for key in
                              ('milestones', 'decisions', 'history_sections', 'history_tasks', 'evidence_references', 'distinct_evidence_locators')}
        report['richness']['distinct_test_references'] = len(test_references)
        write_json(output / 'test-reference-inventory.json', test_references)
        gate('scale_and_domain_richness', preparation['admission']['scale_eligible']
             and preparation['milestones'] >= 2 and preparation['decisions'] >= 2
             and preparation['history_tasks'] > 1 and preparation['distinct_evidence_locators'] > 1
             and len(test_references) > 1, 'Source scale or observed domain richness is missing')
        initialized = run('01-init', 'init', '--manifest', 'project.toml', '--accept')
        initial_status = run('02-status', 'status')
        before = snapshot(project)
        write_json(output / 'before-projection.json', before)
        original_facts = facts(before, project)
        require(initialized['index']['ok'] and initial_status['total'] == len(work)
                and initial_status['counts'] == preparation['status_counts'], 'Runtime inventory differs from the mapped source')
        require(set(original_facts['work_items']) == set(work), 'Runtime changed task identities')
        for key, item in work.items():
            actual = original_facts['work_items'][key]
            require(all(actual[field] == item[field] for field in
                        ('title', 'status', 'acceptance', 'next_action', 'blocker', 'summary')), 'Mapped work fact changed during indexing')
        gate('frozen_source_intake', source_files(project) == preparation['project_files'], 'Indexing rewrote input files')
        run('03-unchanged-reindex', 'source', 'reindex')
        unchanged = snapshot(project)
        require(identities(unchanged) == identities(before) and unchanged['project'] == before['project']
                and facts(unchanged, project) == original_facts, 'Unchanged reindex changed identities, revisions or facts')
        shown = run('04-selected-work', 'work', 'show', selected['id'])
        secondary = run('05-explicit-dependency-work', 'work', 'show', spec['dependency_work'])
        expected_dependencies = set(selected['depends_on'])
        require({item['external_key'] for item in shown['required_dependencies']} == expected_dependencies
                and not shown['missing_dependencies'] and not shown['dependency_cycles'], 'Selected direct dependency facts changed')
        require({item['external_key'] for item in secondary['required_dependencies']} == set(work[spec['dependency_work']]['depends_on']),
                'Independently stated dependency was lost')
        size = len(json.dumps(shown, ensure_ascii=False).encode('utf-8'))
        gate('bounded_work_read', shown['work']['external_key'] == selected['id'] and not shown['work']['ready']
             and all(shown['work'][key] == selected[key] for key in ('status', 'next_action', 'blocker'))
             and shown['acceptance'] == selected['acceptance'] and size < report['scale']['source_bytes'] // 5,
             'Selected work read lost hard facts or expanded the ledger')
        report['work_read_bytes'] = size
        baseline_revision = run('06-baseline-status', 'status')['project_revision']
        # A controlled event contains a verbatim old receipt. It is a benchmark
        # operation now, not a fabricated claim that the original project emitted it.
        old_payload = output / 'historical-probe-payload.json'
        write_json(old_payload, {'source_receipt': historical['summary'], 'probe': 'controlled history-isolation observation'})
        event = run('07-unrelated-history-event', 'event', 'append', '--work', historical['id'],
                    '--type', 'work.progress', '--importance', 'critical', '--summary', historical['title'],
                    '--payload', old_payload, '--expected-revision', baseline_revision)
        context = run('08-current-context', 'context', 'compile', '--work', selected['id'],
                      '--after-revision', baseline_revision, '--budget', 5000)
        full_history = run('09-explicit-historical-work', 'object', 'show', 'work', historical['id'], '--full')
        full_event = run('10-explicit-history-event', 'event', 'show', event['event']['id'], '--full')
        text = context['work_context']['rendered_context']
        require(full_history['object']['summary'] == historical['summary'] and historical['summary'], 'Historical receipt cannot be explicitly retrieved')
        require(full_event['event']['payload']['source_receipt'] == historical['summary'], 'Historical event payload was lost')
        require(all(value in text for value in [selected['next_action'], selected['blocker'], *selected['acceptance']]),
                'Selected hard facts missing from compiled context')
        require(not any(value in json.dumps(context, ensure_ascii=False) for value in
                        [historical['title'], historical['summary'], historical['id']]), 'Unrelated closed work entered current context')
        gate('unrelated_history_isolation', True, 'Unrelated history isolation failed')
        control_next = selected['next_action'] + '\nControlled benchmark edit in a disposable copy; original project remains unchanged.'
        selected['next_action'] = control_next
        (project / 'work-ledger.yaml').write_text(yaml.safe_dump(document, allow_unicode=True, sort_keys=False, width=120), encoding='utf-8')
        report['controlled_source_files'] = source_files(project)
        run('11-incremental-reindex', 'source', 'reindex')
        changed = snapshot(project)
        write_json(output / 'changed-projection.json', changed)
        changed_facts = facts(changed, project)
        expected_facts = json.loads(json.dumps(original_facts))
        expected_facts['edges'] = original_facts['edges']
        expected_facts['evidence'] = original_facts['evidence']
        expected_facts['work_items'][selected['id']]['next_action'] = control_next
        require(changed_facts == expected_facts, 'Incremental indexing changed unrelated source facts')
        old_ids, new_ids = identities(before), identities(changed)
        for table in old_ids:
            require(set(old_ids[table]) == set(new_ids[table]), 'Incremental reindex replaced stable identities')
        selected_id = shown['work']['id']
        changed_entities = [(table, identity) for table in DOMAIN_TABLES
                            for identity in old_ids[table] if old_ids[table][identity] != new_ids[table][identity]]
        require(changed_entities == [('work_items', selected_id)] and
                new_ids['work_items'][selected_id] == old_ids['work_items'][selected_id] + 1,
                'Only the edited task should receive a new domain revision')
        require(source_files(project)['work-ledger.yaml'] != preparation['project_files']['work-ledger.yaml'], 'Controlled source change had no fingerprint delta')
        run('12-repeat-incremental-reindex', 'source', 'reindex')
        repeated = snapshot(project)
        gate('idempotent_and_incremental_index', identities(repeated) == identities(changed)
             and repeated['project'] == changed['project'] and facts(repeated, project) == changed_facts,
             'Repeated incremental index was not idempotent')
        final_context = run('13-changed-context', 'context', 'compile', '--work', selected['id'],
                            '--after-revision', baseline_revision, '--budget', 5000)
        require(control_next in final_context['work_context']['rendered_context'], 'Changed next action missing from new context')
        require(historical['title'] not in json.dumps(final_context, ensure_ascii=False), 'Source delta reintroduced unrelated history')
        rebuilt = output / 'rebuilt-project'
        shutil.copytree(prepared / 'project', rebuilt)
        shutil.copyfile(project / 'work-ledger.yaml', rebuilt / 'work-ledger.yaml')
        require(not (rebuilt / '.awr').exists(), 'Rebuild must start without a copied runtime database')
        rebuilt_init = run('14-independent-rebuild', 'init', '--manifest', 'project.toml', '--accept', target=rebuilt)
        rebuilt_state = snapshot(rebuilt)
        write_json(output / 'rebuilt-projection.json', rebuilt_state)
        require(rebuilt_init['index']['ok'] and rebuilt_state['project']['id'] != before['project']['id'], 'Rebuild reused project identity')
        require(all(event['event_type'] != 'work.progress' for event in rebuilt_state['events']), 'Rebuild fabricated runtime work history')
        gate('projection_rebuild', facts(rebuilt_state, rebuilt) == changed_facts
             and not any(rebuilt_state['runtime_counts'].values()) and source_files(rebuilt) == source_files(project),
             'Independent rebuild changed source facts or invented runtime records')
        run('15-doctor', 'doctor')
        run('16-rebuild-doctor', 'doctor', target=rebuilt)
        gate('original_source_unchanged', source_state(intake) == preparation['source_state']
             and source_files(prepared / 'project') == preparation['project_files']
             and digest(prepared / 'mapping-spec.json') == preparation['mapping_sha256'], 'Frozen or original sources changed')
        report.update(project_id=before['project']['id'], rebuilt_project_id=rebuilt_state['project']['id'],
                      changed_domain_entities=len(changed_entities), original_sources_unchanged=True,
                      final_context_token_estimate=final_context['work_context']['token_estimate'])
        public = {key: report[key] for key in ('kind', 'contract_id', 'contract_version', 'source_commit',
                  'checked_at', 'environment', 'binary_sha256', 'preparation_sha256', 'mapping_sha256',
                  'source_map_sha256', 'original_source_commit', 'scale', 'richness', 'work_read_bytes',
                  'changed_domain_entities', 'original_sources_unchanged', 'e4_credit', 'metric_credit',
                  'model_invocations', 'original_backend_commands_executed')}
        public.update(gates={**report['gates'], 'public_evidence_redaction': True}, passed=True,
                      command_count=len(report['commands']), commands=[row['label'] for row in report['commands']],
                      mapping_limits={'unresolved_phase_labels': len(preparation['unresolved_phase_labels']),
                                      'dependency_graph': 'explicit selected-work prerequisites and one additional driver edge; remaining edges unspecified',
                                      'historical_verification': 'source references retained; original backend validations not rerun'},
                      scope='Native local large-ledger benchmark; no real-client E4, token metric, latency metric or release credit.')
        packed = json.dumps(public, ensure_ascii=False)
        prohibited = [str(ROOT), str(intake), preparation['intake_path'], *work.keys(),
                      *[item['title'] for item in work.values() if len(item['title']) >= 8]]
        gate('public_evidence_redaction', not any(value in packed for value in prohibited), 'Private source identity entered public evidence')
        require(set(report['gates']) == set(contract['required_gates']) and all(report['gates'].values()), 'Missing contract gates')
        require(git(ROOT, 'rev-parse', 'HEAD') == report['source_commit'] and
                not git(ROOT, 'status', '--porcelain', '--untracked-files=all') and digest(binary) == report['binary_sha256'],
                'Execution inputs changed during the run')
        report['passed'] = True
        save()
        public['private_report_sha256'] = digest(output / 'report.json')
        write_json(output / 'public-summary.json', public)
        print(json.dumps({'passed': True, 'gates_passed': len(report['gates']), 'commands': len(report['commands']),
                          'e4_credit': 0, 'metric_credit': 0}))
    except Exception as error:
        report['failure'] = str(error)
        save()
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--awr', type=Path, required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepared, output = args.prepared.resolve(strict=True), args.output.resolve()
    prepared.relative_to(ROOT / '.local')
    output.relative_to(ROOT / '.local')
    require(not output.exists(), 'Use a new run directory and preserve old failure receipts')
    run_benchmark(args.awr.resolve(strict=True), prepared, output)


if __name__ == '__main__':
    main()
