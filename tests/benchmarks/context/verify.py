#!/usr/bin/env python3
"""Measure frozen real-source L0/L1 output with an independent exact tokenizer."""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
from urllib.parse import urlsplit
from urllib.request import url2pathname

import yaml

from oracle import assess

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'tests/long-ledger'))
from prepare import Mapping, digest, source_state, write_json
from inspect_sample import inspect_bytes, require


def command(arguments):
    prefix = ['rtk', 'proxy'] if shutil.which('rtk') else []
    return subprocess.run(prefix + list(map(str, arguments)), cwd=ROOT, capture_output=True,
                          text=True, encoding='utf-8', timeout=60)


def git(*arguments):
    result = command(['git', *arguments])
    require(result.returncode == 0, 'Cannot verify benchmark source revision')
    return result.stdout.strip()


def authority_files(project):
    return sorted(p for p in project.rglob('*') if p.is_file() and '.awr' not in p.parts
                  and p.suffix in {'.md', '.yaml'})


def hashes(project):
    return {p.relative_to(project).as_posix(): digest(p) for p in authority_files(project)}


def source_oracle(project, intake, spec):
    ledger = yaml.safe_load((project / 'work-ledger.yaml').read_text(encoding='utf-8'))
    works = {w['id']: w for w in ledger['work_items']}
    work = works[spec['selected_work']]
    mapping = Mapping(intake, spec)
    rules = []
    for rule in spec['rules']:
        if rule['scope'] == 'project' or (rule['scope'] == 'work_item' and rule['value'] == work['id']):
            rules.append({'key': rule['id'], 'text': mapping.read(rule['title']) + '\n\n' + mapping.read(rule['body'])})
    require(rules and work['acceptance'] and work['next_action'] and work.get('blocker'), 'The frozen real task lost required facts')
    pending = [works[key] for key in work['depends_on'] if works[key]['status'] != 'completed']
    return {'work': work, 'rules': rules, 'unresolved_dependencies': pending}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--awr', type=Path, required=True)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--tokenizer-cache', type=Path, required=True)
    args = parser.parse_args()
    binary, prepared = args.awr.resolve(strict=True), args.prepared.resolve(strict=True)
    output, cache = args.output.resolve(), args.tokenizer_cache.resolve()
    for path in [prepared, output, cache]:
        path.relative_to(ROOT / '.local')
    require(not output.exists(), 'Use a new output directory; retain previous failures')
    require(not git('status', '--porcelain', '--untracked-files=all'), 'Commit benchmark inputs before execution')
    contract = json.loads((HERE / 'contract.json').read_text(encoding='utf-8'))
    preparation = json.loads((prepared / 'preparation.json').read_text(encoding='utf-8'))
    for name, key in [('preparation.json', 'preparation_sha256'), ('mapping-spec.json', 'mapping_sha256'), ('source-map.json', 'source_map_sha256')]:
        require(digest(prepared / name) == contract['sample'][key], 'The frozen real sample binding changed')
    for name, value in preparation['project_files'].items():
        require(digest(prepared / 'project' / name) == value, 'Prepared source file changed')
    intake = Path(preparation['intake_path']).resolve(strict=True)
    original = source_state(intake)
    spec = json.loads((prepared / 'mapping-spec.json').read_text(encoding='utf-8'))
    admission = inspect_bytes((prepared / 'project/work-ledger.yaml').read_bytes(),
        json.loads((ROOT / contract['sample']['admission_contract']).read_text()))
    require(admission['scale_eligible'], 'Real source no longer meets the large-ledger admission')
    os.environ['TIKTOKEN_CACHE_DIR'] = str(cache)
    import tiktoken
    require('tiktoken==' + tiktoken.__version__ == contract['tokenizer']['independent_python_package'], 'Independent tokenizer version drift')
    import tomllib
    packages = tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']
    require(any(p['name'] + '==' + p['version'] == contract['tokenizer']['native_rust_package'] for p in packages), 'Native tokenizer version drift')
    enc = tiktoken.get_encoding(contract['tokenizer']['encoding'])
    count = lambda text: len(enc.encode_ordinary(text))
    inputs = {p.relative_to(ROOT).as_posix(): digest(p) for p in sorted(HERE.iterdir()) if p.is_file()}
    output.mkdir(parents=True)
    report = {'kind': 'context_token_and_recall_benchmark', 'contract_id': contract['contract_id'],
              'version': contract['version'], 'source_commit': git('rev-parse', 'HEAD'),
              'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'),
              'binary_sha256': digest(binary), 'inputs': inputs, 'sample': contract['sample'],
              'environment': {'system': platform.system(), 'machine': platform.machine(), 'python': platform.python_version()},
              'tokenizer': contract['tokenizer'], 'tokenizer_cache_sha256': {p.name: digest(p) for p in sorted(cache.iterdir()) if p.is_file()},
              'passed': False, 'gates': {'frozen_real_source': True}, 'cases': [],
              'e4_completed': 0, 'model_invocations': 0}

    def save():
        write_json(output / 'report.json', report)

    save()
    cases = ['baseline-' + str(i + 1) for i in range(contract['baseline_repetitions'])] + ['pending-dependency']
    for case_id in cases:
        base = output / case_id
        project = base / 'project'
        receipts = base / 'receipts'
        receipts.mkdir(parents=True)
        shutil.copytree(prepared / 'project', project)
        row = {'case': case_id, 'passed': False, 'commands': []}
        report['cases'].append(row)
        save()
        if case_id == 'pending-dependency':
            path = project / 'work-ledger.yaml'
            ledger = yaml.safe_load(path.read_text(encoding='utf-8'))
            selected = next(w for w in ledger['work_items'] if w['id'] == spec['selected_work'])
            dependency = next(w for w in ledger['work_items'] if w['id'] == sorted(selected['depends_on'])[0])
            require(dependency['status'] == 'completed', 'Coverage probe must start with an existing completed prerequisite')
            row['controlled_change'] = {'work': dependency['id'], 'before': {k: dependency.get(k) for k in ['status', 'blocker', 'next_action']}}
            dependency.update({k: contract['controlled_dependency_probe'][k] for k in ['status', 'blocker', 'next_action']})
            path.write_text(yaml.safe_dump(ledger, allow_unicode=True, sort_keys=False, width=120), encoding='utf-8')
        before = hashes(project)
        oracle = source_oracle(project, intake, spec)
        write_json(base / 'oracle.json', oracle)
        row['oracle_sha256'] = digest(base / 'oracle.json')
        corpus = '\n'.join(p.read_text(encoding='utf-8') for p in authority_files(project))
        row['baseline'] = {'source_files': before, 'source_bytes': sum(p.stat().st_size for p in authority_files(project)),
                           'joined_utf8_bytes': len(corpus.encode('utf-8')), 'source_tokens': count(corpus),
                           'corpus_sha256': hashlib.sha256(corpus.encode('utf-8')).hexdigest()}

        def run(label, *arguments, error=None):
            result = command([binary, '--project', project, '--json', *arguments])
            receipt = receipts / (label + '.json')
            require(not receipt.exists(), 'Duplicate command label')
            write_json(receipt, {'arguments': list(map(str, arguments)), 'exit_code': result.returncode,
                                 'stdout': result.stdout, 'stderr': result.stderr})
            row['commands'].append({'label': label, 'exit_code': result.returncode,
                                    'receipt': receipt.relative_to(output).as_posix(), 'sha256': digest(receipt),
                                    'stdout_utf8_bytes': len(result.stdout.encode('utf-8')), 'stdout_tokens': count(result.stdout)})
            save()
            require((result.returncode == 0) == (error is None), case_id + ': unexpected result for ' + label)
            body = json.loads(result.stderr if error else result.stdout)
            if error:
                require(body['code'] == error and not result.stdout, 'Budget rejection returned a usable partial success')
            return body

        init = run('01-init', 'init', '--manifest', 'project.toml', '--accept')
        require(init['index']['ok'], 'Real source did not index')
        revision = run('02-status', 'status')['project_revision']
        work = run('03-source-work', 'object', 'show', 'work', oracle['work']['id'], '--full')['object']
        bootstrap_args = ['context', 'bootstrap', '--work', oracle['work']['id'], '--budget', contract['budgets']['bootstrap']]
        goal_args = [v for g in oracle['work']['goals'] for v in ['--goal', g]]
        compile_args = ['context', 'compile', '--work', oracle['work']['id'], *goal_args, '--after-revision', revision,
                        '--budget', contract['budgets']['work_context']]
        bootstrap = run('04-bootstrap', *bootstrap_args)
        compiled = run('05-context', *compile_args)
        require(bootstrap['context']['complete'] and compiled['completeness']['complete'], 'Required context facts are incomplete')
        require(run('06-bootstrap-repeat', *bootstrap_args) == bootstrap and run('07-context-repeat', *compile_args) == compiled,
                'Unchanged source context is not stable across fresh processes')
        row['recall'] = assess(oracle, work, bootstrap, compiled)
        require(set(row['recall']['categories']) == set(contract['hard_fact_categories']), 'Hard fact category coverage changed')
        require(row['recall']['recall'] == 1, 'A mandatory source fact was lost')
        source_versions = {s['id']: s for s in bootstrap['context']['source_revisions']}
        for entity in [bootstrap['context']['work'], *bootstrap['context']['critical_rules']]:
            ref = entity['source_ref']
            source = source_versions[ref['source_id']]
            require(source['revision'] == ref['source_revision'] and source['fingerprint'] == ref['source_fingerprint']
                    and source['locator'] == ref['locator'] and source['freshness'] == 'fresh', 'L0 lost a complete provenance binding')
            relative = Path(url2pathname(urlsplit(ref['locator']).path)).relative_to(project).as_posix()
            require(source['fingerprint'] == 'sha256:' + before[relative], 'Provenance no longer resolves to exact source bytes')
        row['measurements'] = {}
        for name, pack in [('bootstrap', bootstrap), ('work_context', compiled['work_context'])]:
            native, actual = pack['token_estimate'], count(pack['rendered_context'])
            row['measurements'][name] = {'native_legacy_token_estimate': native, 'independent_actual_tokens': actual,
                'rendered_utf8_bytes': len(pack['rendered_context'].encode('utf-8')), 'budget': pack['token_budget'],
                'tokenizer': pack['tokenizer'], 'context_hash': pack['context_hash']}
            require(pack['tokenizer'] == contract['tokenizer']['encoding'] and native == actual, 'Independent encoding count differs')
            require(actual <= contract['budgets'][name], 'Token budget exceeded')
        row['measurements']['compression_ratio'] = row['baseline']['source_tokens'] / row['measurements']['work_context']['independent_actual_tokens']
        require(row['measurements']['compression_ratio'] > contract['thresholds']['compression_ratio_exclusive_minimum'], 'Compression threshold failed')
        row['unresolved_dependency_count'] = len(oracle['unresolved_dependencies'])
        row['context_gaps'] = compiled['completeness']['issues']
        row['evidence_gaps'] = compiled['completeness']['evidence_gaps']
        row['omitted_chunks'] = compiled['work_context']['omitted_chunks']
        row['overflow_probes'] = []
        for label, arguments in [('08-bootstrap-overflow', bootstrap_args), ('09-context-overflow', compile_args)]:
            rejected = run(label, *arguments[:-1], 1, error='BudgetExceeded')
            row['overflow_probes'].append({'label': label, 'error': rejected})
        require(hashes(project) == before, 'Runtime context reads changed authority source files')
        row.update(passed=True, deterministic=True, provenance_complete=True, source_unchanged=True)
        save()

    require(source_state(intake) == original, 'Original real project changed during benchmark')
    for name, value in preparation['project_files'].items():
        require(digest(prepared / 'project' / name) == value, 'Frozen sample changed during execution')
    require(all(digest(ROOT / name) == value for name, value in inputs.items()), 'Benchmark inputs changed during execution')
    require(report['cases'][-1]['unresolved_dependency_count'] > 0, 'No nonempty required dependency was exercised')
    report['metrics'] = {
        'bootstrap_tokens': max(c['measurements']['bootstrap']['independent_actual_tokens'] for c in report['cases']),
        'work_context_tokens': max(c['measurements']['work_context']['independent_actual_tokens'] for c in report['cases']),
        'context_compression_ratio': min(c['measurements']['compression_ratio'] for c in report['cases']),
        'hard_fact_recall': sum(c['recall']['passed'] for c in report['cases']) / sum(c['recall']['assertions'] for c in report['cases'])}
    report['gates'] = {name: True for name in contract['required_gates']}
    report['passed'] = True
    save()
    public_cases = []
    for row in report['cases']:
        public_cases.append({k: row[k] for k in ['case', 'passed', 'oracle_sha256', 'measurements', 'recall', 'unresolved_dependency_count']})
        public_cases[-1].update(source_tokens=row['baseline']['source_tokens'], source_file_count=len(row['baseline']['source_files']),
                               corpus_sha256=row['baseline']['corpus_sha256'], calls=len(row['commands']),
                               complete_json_transport=[{k: cmd[k] for k in ['label', 'stdout_utf8_bytes', 'stdout_tokens']}
                                                        for cmd in row['commands'] if cmd['label'] in ['04-bootstrap', '05-context']])
    summary = {k: report[k] for k in ['kind', 'contract_id', 'version', 'source_commit', 'checked_at', 'binary_sha256', 'inputs',
               'sample', 'environment', 'tokenizer', 'tokenizer_cache_sha256', 'passed', 'gates', 'metrics', 'e4_completed', 'model_invocations']}
    summary.update(cases=public_cases, private_report_sha256=digest(output / 'report.json'),
                   limitations=[contract['boundary'], contract['aggregation'], '50–100x compression is a stretch target, not achieved by a >20x result.'])
    write_json(output / 'public-summary.json', summary)
    print(json.dumps({'passed': True, 'cases': len(cases), 'gates': len(report['gates']), 'metrics': report['metrics'],
                      'assertions': sum(c['recall']['assertions'] for c in report['cases'])}, indent=2))


if __name__ == '__main__':
    main()
