#!/usr/bin/env python3
"""Exercise a frozen recovery contract through fresh CLI processes and retain every receipt."""
import argparse
from contextlib import closing
from datetime import datetime
import hashlib
import json
from pathlib import Path
import platform
import shutil
import sqlite3
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def command(arguments):
    prefix = ['rtk', 'proxy'] if shutil.which('rtk') else []
    return subprocess.run(prefix + list(map(str, arguments)), cwd=ROOT, capture_output=True,
                          text=True, encoding='utf-8', timeout=60)


def git(*arguments):
    result = command(['git', *arguments])
    require(result.returncode == 0, 'Cannot bind benchmark source revision')
    return result.stdout.strip()


def frozen_inputs():
    paths = [HERE / 'contract.json', HERE / 'verify.py', *sorted((HERE / 'fixture').rglob('*'))]
    return {p.relative_to(ROOT).as_posix(): digest(p) for p in paths if p.is_file()}


def source_hashes(project):
    return {p.name: digest(p) for p in sorted(project.iterdir()) if p.is_file()}


def runtime_records(project):
    """Logical read-only snapshot; source refresh is allowed during a rejected resume."""
    uri = (project / '.awr/state.db').as_uri() + '?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        db.execute('BEGIN')
        return {table: sorted(db.execute('SELECT * FROM ' + table).fetchall())
                for table in ['sessions', 'claims', 'checkpoints']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--awr', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.awr.resolve(strict=True)
    output = args.output.resolve()
    output.relative_to(ROOT / '.local')
    require(not output.exists(), 'Use a new output directory; preserve prior failure evidence')
    require(not git('status', '--porcelain', '--untracked-files=all'), 'Commit the benchmark inputs before running')
    contract = json.loads((HERE / 'contract.json').read_text(encoding='utf-8'))
    inputs = frozen_inputs()
    output.mkdir(parents=True)
    report = {'kind': 'compact_recovery_benchmark', 'contract_id': contract['contract_id'],
              'contract_version': contract['version'], 'source_commit': git('rev-parse', 'HEAD'),
              'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'),
              'environment': {'system': platform.system(), 'machine': platform.machine(), 'python': platform.python_version()},
              'binary_sha256': digest(binary), 'inputs': inputs, 'passed': False, 'cases': [],
              'e4_completed': 0, 'metric_credit': 0, 'model_invocations': 0}

    def save():
        write_json(output / 'report.json', report)

    save()
    e = contract['expectations']
    for case in contract['cases']:
        base = output / case['id']
        project = base / 'project'
        receipts = base / 'receipts'
        receipts.mkdir(parents=True)
        shutil.copytree(HERE / 'fixture', project)
        row = {'id': case['id'], 'passed': False, 'gates': {}, 'commands': [], 'faults': []}
        report['cases'].append(row)
        save()

        def run(label, *arguments, error=None):
            result = command([binary, '--project', project, '--json', *arguments])
            receipt = receipts / (label + '.json')
            require(not receipt.exists(), 'Duplicate command label')
            write_json(receipt, {'arguments': list(map(str, arguments)), 'exit_code': result.returncode,
                                 'stdout': result.stdout, 'stderr': result.stderr})
            row['commands'].append({'label': label, 'exit_code': result.returncode,
                                    'receipt': receipt.relative_to(output).as_posix(), 'sha256': digest(receipt)})
            save()
            require((result.returncode == 0) == (error is None), case['id'] + ': unexpected result for ' + label)
            body = json.loads(result.stdout if error is None else result.stderr)
            if error is not None:
                require(body['code'] == error, case['id'] + ': wrong error for ' + label)
            return body

        def revision(label):
            return run(label, 'status')['project_revision']

        initial = run('01-init', 'init', '--manifest', 'project.toml', '--accept')
        require(initial['index']['ok'], 'Fixture did not index')
        started = run('02-start', 'session', 'start', '--work', e['work'], '--agent', 'drafting-editor',
                      '--provider', 'fixture', '--model', 'no-model-invocation', '--claim', '--ttl-ms', 3600000,
                      '--expected-revision', revision('02-status'))
        sid = started['session']['id']
        original_work = run('03-work', 'object', 'show', 'work', e['work'], '--full')['object']
        run('04-progress-event', 'event', 'append', '--work', e['work'], '--session', sid,
            '--type', 'work.progress', '--importance', 'high', '--summary', 'Drafted the first handover outline.',
            '--expected-revision', started['project_revision'])
        cp = None

        def change_sources():
            path = project / 'work-ledger.yaml'
            ledger = yaml.safe_load(path.read_text(encoding='utf-8'))
            work = next(w for w in ledger['work_items'] if w['id'] == e['work'])
            require(work['next_action'] == e['original_next_action'], 'Source change must apply exactly once')
            work['next_action'] = e['current_next_action']
            path.write_text(yaml.safe_dump(ledger, allow_unicode=True, sort_keys=False), encoding='utf-8')
            rules = project / 'RULES.md'
            text = rules.read_text(encoding='utf-8')
            require(text.count(e['original_common_rule']) == 1, 'Frozen common rule does not match the contract')
            rules.write_text(text.replace(e['original_common_rule'], e['current_common_rule']), encoding='utf-8')
            row['controlled_source_hashes'] = source_hashes(project)
            save()

        if case['source_change'] == 'before_checkpoint':
            change_sources()
        if case['checkpoint']:
            used = run('05-used-context', 'context', 'compile', '--session', sid, '--budget', contract['budgets']['work_context'])
            cp = run('06-checkpoint', 'session', 'checkpoint', '--session', sid,
                     '--context-hash', used['work_context']['context_hash'], '--digest', 'The first handover outline is drafted.',
                     '--next-action', e['saved_next_action'], '--open-loop', e['open_loops'][0],
                     '--open-loop', e['open_loops'][1], '--expected-revision', used['project_revision'])
            require(cp['checkpoint_save']['delta_recorded'], 'Checkpoint did not save observed delta')

        # An unrelated event occurs after the saved baseline, so time filtering alone cannot exclude it.
        write_json(project / 'old-history.json', {'body': e['unrelated_payload']})
        unrelated = run('07-unrelated-event', 'event', 'append', '--work', e['unrelated_work'],
                        '--type', 'work.progress', '--importance', 'critical', '--summary', e['unrelated_summary'],
                        '--payload', 'old-history.json', '--expected-revision', revision('07-status'))
        before_change_revision = unrelated['project_revision']
        if case['source_change'] != 'before_checkpoint':
            change_sources()

        def resume(label, expected, error=None):
            return run(label, 'session', 'resume', '--from-session', sid, '--agent', 'receiving-editor',
                       '--provider', 'fixture', '--model', 'no-model-invocation', '--expected-revision', expected,
                       '--budget', contract['budgets']['work_context'], error=error)

        if case['fault'] == 'stale_revision':
            before = runtime_records(project)
            resume('08-rejected-stale-revision', before_change_revision, error='RevisionConflict')
            require(runtime_records(project) == before, 'Rejected stale resume altered durable runtime records')
            row['faults'].append({'code': 'RevisionConflict', 'runtime_records_unchanged': True})
        elif case['fault'] == 'missing_source':
            before = runtime_records(project)
            rules = project / 'RULES.md'
            missing = project / 'RULES.md.offline'
            rules.rename(missing)
            resume('08-rejected-missing-source', before_change_revision, error='SourceStale')
            require(runtime_records(project) == before, 'Rejected missing-source resume altered durable runtime records')
            missing.rename(rules)
            row['faults'].append({'code': 'SourceStale', 'runtime_records_unchanged': True})

        before_recovery_sources = source_hashes(project)
        resumed = resume('09-resume', revision('09-refreshed-status'))
        require(resumed['context_ready'], 'Recovery returned incomplete work context')
        successor = resumed['resumed']['session']
        nid = successor['id']
        require(nid != sid and successor['work_item_id'] == started['session']['work_item_id'], 'Resume changed the selected work')
        require(successor['agent_id'] == 'receiving-editor' and successor['last_checkpoint_id'] is None,
                'Resume did not create a distinct receiving session')
        require(resumed['resumed']['from_session']['status'] == 'interrupted', 'Exited caller was not retained as interrupted')
        require(resumed['resumed']['claim']['id'] != started['claim']['id'] and
                resumed['resumed']['closed_claim_ids'] == [started['claim']['id']], 'Live claim was not transferred atomically')
        shown = run('10-new-process-session', 'session', 'show', nid)
        require(shown['session'] == successor, 'New process lost the successor')
        row['gates']['separate_processes_and_durable_sessions'] = True

        bootstrap = run('11-bootstrap', 'context', 'bootstrap', '--session', nid, '--budget', contract['budgets']['bootstrap'])
        baseline = ['--checkpoint', cp['checkpoint']['id']] if cp else ['--after-revision', resumed['context']['delta_after_revision']]
        context = run('12-recompiled-context', 'context', 'compile', '--work', e['work'], '--session', nid,
                      '--agent', 'receiving-editor', '--intent', 'resume', *baseline,
                      '--budget', contract['budgets']['work_context'])
        require(context == resumed['context'], 'Unchanged context differs across processes')
        current = run('13-current-work', 'object', 'show', 'work', e['work'], '--full')['object']
        text = context['work_context']['rendered_context']
        require(current['id'] == original_work['id'] and current['acceptance'] == [e['acceptance']]
                and e['acceptance'] in text and bootstrap['context']['work']['external_key'] == e['work'],
                'Current work identity or exact acceptance was lost')
        row['gates']['exact_current_work_and_acceptance'] = True
        require(current['next_action'] == e['current_next_action'] and e['current_next_action'] in text
                and bootstrap['context']['work']['next_action'] == e['current_next_action'], 'New source next action was lost')
        rules = {rule['text'] for rule in bootstrap['context']['critical_rules']}
        require(rules == {e['current_common_rule'], e['receiver_rule']}, 'Receiver hard-rule scope differs from the contract')
        require(all(rule in text for rule in rules) and e['sender_rule'] not in text, 'L1 contains missing or inapplicable hard rules')
        row['gates']['current_rules_and_receiver_scope'] = True

        delta = run('14-delta', 'context', 'delta', '--session', nid)
        if cp:
            saved = run('15-saved-checkpoint', 'object', 'show', 'checkpoint', cp['checkpoint']['id'], '--full')
            require(bootstrap['context']['checkpoint'] == cp['checkpoint'] and
                    resumed['checkpoint_id'] == cp['checkpoint']['id'], 'Checkpoint changed across recovery')
            require(all(loop in text for loop in e['open_loops']) and e['saved_next_action'] in text,
                    'Saved next action or open loops were dropped')
            require(saved['object'] == cp['checkpoint'] and saved['session_delta']['session_events'], 'Saved progress observations were lost')
            if case['source_change'] == 'before_checkpoint':
                observed = saved['session_delta']['source_observations']
                changed = {v['external_key'] for s in observed for v in s['changes']}
                require({e['work'], 'facts'} <= changed, 'Checkpoint omitted observed source changes')
                require(saved['object']['changed_entities'] and all(item in bootstrap['rendered_context']
                        for item in saved['object']['changed_entities']), 'Bootstrap omitted saved changed identities')
            else:
                changed = {v['external_key'] for s in delta['delta']['events']['source_changes'] for v in s['changed_entities']}
                require({e['work'], 'facts'} <= changed and 'source.projected' in text, 'Post-checkpoint source delta missing')
        else:
            require(resumed['checkpoint_id'] is None and resumed['checkpoint_save'] is None
                    and bootstrap['context']['checkpoint'] is None and shown['inherited_checkpoint'] is None,
                    'Checkpointless recovery fabricated a saved checkpoint')
            require(resumed['recovery_basis'] == 'source_and_session_start' and resumed['recovery_gaps'], 'Unsaved state was not disclosed')
            require(e['saved_next_action'] not in text and not any(loop in text for loop in e['open_loops']), 'Unsaved memory was invented')
            require(delta['delta']['after_revision'] == started['session']['start_project_revision']
                    and 'source.projected' in text, 'Checkpointless source baseline was lost')
            row['faults'].append({'code': 'NoSuccessfulCheckpoint', 'gaps': resumed['recovery_gaps'], 'fabricated_state': False})
        row['gates']['current_and_saved_next_actions_distinguished'] = True
        row['gates']['saved_open_loops_or_explicit_recovery_gap'] = True
        row['gates']['source_delta_and_saved_observations'] = True

        packed = json.dumps([resumed, bootstrap, context, delta], ensure_ascii=False)
        require(not any(e[key] in packed for key in ['unrelated_work', 'unrelated_title', 'unrelated_summary', 'unrelated_payload']),
                'Unrelated closed history entered recovery context')
        historical = run('16-explicit-history', 'event', 'show', unrelated['event']['id'], '--full')
        require(e['unrelated_summary'] in json.dumps(historical) and e['unrelated_payload'] in json.dumps(historical),
                'Excluded history was destroyed instead of remaining retrievable')
        row['gates']['unrelated_history_excluded_but_retrievable'] = True
        before = runtime_records(project)
        resume('17-duplicate-resume', context['project_revision'], error='InvalidTransition')
        require(runtime_records(project) == before, 'Duplicate resume changed runtime records')
        row['faults'].append({'code': 'InvalidTransition', 'runtime_records_unchanged': True})
        row['gates']['fault_receipt_and_retry_without_duplicate_successor'] = True
        run('18-end', 'session', 'end', '--session', nid, '--outcome', 'incomplete',
            '--expected-revision', context['project_revision'])
        doctor = run('19-doctor', 'doctor')
        require(doctor['ok'] and not doctor['findings'], 'Recovery left unresolved runtime findings')
        require(source_hashes(project) == before_recovery_sources and frozen_inputs() == inputs,
                'Recovery rewrote sources or the frozen fixture')
        row['gates']['unchanged_fixture_and_read_only_recovery_sources'] = True
        require(set(row['gates']) == set(contract['required_gates']) and all(row['gates'].values()), 'Incomplete gate coverage')
        row.update(passed=True, session_a=sid, session_b=nid, project_id=successor['project_id'],
                   work_id=current['id'], checkpoint_id=resumed['checkpoint_id'],
                   bootstrap_token_estimate=bootstrap['token_estimate'],
                   work_context_token_estimate=context['work_context']['token_estimate'],
                   source_hashes_after_recovery=source_hashes(project), doctor_findings=0)
        save()
    require(len({row['project_id'] for row in report['cases']}) == len(contract['cases']), 'Cases reused runtime identity')
    require(git('rev-parse', 'HEAD') == report['source_commit'] and not git('status', '--porcelain', '--untracked-files=all')
            and frozen_inputs() == inputs and digest(binary) == report['binary_sha256'], 'Execution inputs changed during the run')
    report.update(passed=True, source_unchanged=True, cases_passed=len(report['cases']),
                  gate_assertions=sum(len(row['gates']) for row in report['cases']))
    save()
    print(json.dumps({k: report[k] for k in ['passed', 'cases_passed', 'gate_assertions', 'e4_completed', 'metric_credit']}))


if __name__ == '__main__':
    main()
