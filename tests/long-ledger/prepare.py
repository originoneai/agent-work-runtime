#!/usr/bin/env python3
"""Map a reviewed, private Markdown intake to existing AWR source adapters.

This is benchmark preparation, not a new runtime Markdown ledger adapter. The
private mapping is explicit, hashed and retained with the original source bytes.
No commands from source documents are executed and no original file is written.
"""
import argparse
from collections import Counter
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

import yaml

from inspect_sample import inspect_bytes, require

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def digest_bytes(body):
    return hashlib.sha256(body).hexdigest()


def digest(path):
    return digest_bytes(path.read_bytes())


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def git(repository, *arguments, binary=False):
    prefix = ['rtk', 'proxy'] if shutil.which('rtk') else []
    result = subprocess.run(prefix + ['git', '-C', str(repository), *arguments],
                            capture_output=True, timeout=30)
    require(result.returncode == 0, 'Cannot verify source Git provenance')
    return result.stdout if binary else result.stdout.decode('utf-8').strip()


def source_state(intake_dir):
    intake = json.loads((intake_dir / 'intake.json').read_text(encoding='utf-8'))
    repository = Path(intake['source_repository']).resolve(strict=True)
    require(git(repository, 'rev-parse', 'HEAD') == intake['source_commit'], 'Source checkout moved')
    require(not git(repository, 'status', '--porcelain', '--untracked-files=all'), 'Source checkout is not clean')
    hashes = {}
    for source in intake['sources']:
        original = Path(source['source_path']).resolve(strict=True)
        relative = original.relative_to(repository).as_posix()
        frozen = (intake_dir / source['frozen_path']).resolve(strict=True)
        frozen.relative_to(intake_dir / 'originals')
        body = original.read_bytes()
        require(body == frozen.read_bytes() == git(repository, 'show', intake['source_commit'] + ':' + relative, binary=True),
                'Original, frozen copy and Git blob differ')
        require(len(body) == source['bytes'] and digest_bytes(body) == source['sha256'], 'Intake fingerprint differs')
        hashes[relative] = source['sha256']
    return {'source_commit': intake['source_commit'], 'source_worktree_clean': True, 'hashes': hashes}


class Mapping:
    def __init__(self, intake_dir, spec):
        self.base = (intake_dir / 'originals').resolve(strict=True)
        self.spec = spec
        self.files = {}
        intake = json.loads((intake_dir / 'intake.json').read_text(encoding='utf-8'))
        for item in intake['sources']:
            path = (intake_dir / item['frozen_path']).resolve(strict=True)
            relative = path.relative_to(self.base).as_posix()
            self.files[relative] = path.read_text(encoding='utf-8').splitlines()
        self.references = []

    def read(self, reference):
        path, start, end = reference['file'], reference['start'], reference['end']
        require(path in self.files and type(start) is int and type(end) is int
                and 1 <= start <= end <= len(self.files[path]), 'Invalid source span')
        text = '\n'.join(self.files[path][start - 1:end])
        if 'cell' in reference:
            require(start == end and text.startswith('|') and text.endswith('|'), 'Cell needs one table row')
            cells = [part.strip() for part in text[1:-1].split('|')]
            require(type(reference['cell']) is int and 0 <= reference['cell'] < len(cells), 'Invalid cell index')
            text = cells[reference['cell']]
        if 'substring' in reference:
            require(reference['substring'] and text.count(reference['substring']) == 1, 'Expected source phrase differs')
            text = reference['substring']
        if reference.get('strip_list_marker'):
            text = re.sub(r'^- ', '', text)
        require(digest_bytes(text.encode('utf-8')) == reference['sha256'], 'Source field fingerprint differs')
        self.references.append(reference)
        return text

    def field(self, value):
        if 'ref' in value:
            return self.read(value['ref'])
        require('value' in value and value.get('basis') and value.get('interpretation'),
                'Normalized values need source evidence and an explicit interpretation')
        for reference in value['basis']:
            self.read(reference)
        return value['value']


def mapped_project(intake_dir, spec, output):
    mapping = Mapping(intake_dir, spec)
    inventory = json.loads((intake_dir / 'work-inventory.json').read_text(encoding='utf-8'))
    require(spec['version'] == 1 and isinstance(inventory, list), 'Unsupported mapping or inventory version')
    layouts = {item['file']: item for item in spec['tables']}
    expected = {row['external_key']: row for row in inventory}
    require(len(expected) == len(inventory), 'Duplicate inventory keys')
    seen = set()
    for path, layout in layouts.items():
        mapping.read(layout['status_legend'])
        for number, line in enumerate(mapping.files[path], 1):
            if not line.startswith('|') or not line.endswith('|'):
                continue
            cells = [part.strip() for part in line[1:-1].split('|')]
            if not re.fullmatch(layout['key_pattern'], cells[0]):
                continue
            require(len(cells) == layout['columns'] and cells[0] not in seen, 'Duplicate or malformed task table row')
            key = cells[0]
            require(key in expected, 'Task row is missing from the frozen inventory')
            item = expected[key]
            require(item['source_file'] == path and item['source_line'] == number
                    and item['source_cells'] == cells and item['source_line_sha256'] == digest_bytes(line.encode('utf-8')),
                    'Inventory and original task table disagree')
            require(cells[1] in spec['status_mapping'], 'Unknown source status; no task is silently skipped')
            seen.add(key)
    require(seen == set(expected), 'Every inventory task must have one verified source row')
    milestone_ids = {item['id'] for item in spec['milestones']}
    require(len(milestone_ids) == len(spec['milestones']), 'Duplicate milestone identifiers')
    document = {'milestones': [], 'goals': [], 'work_items': []}
    for kind in ['milestones', 'goals']:
        for item in spec[kind]:
            document[kind].append({'id': item['id'], **{field: mapping.field(value) for field, value in item['fields'].items()}})
    source_map = {'version': 1, 'work_items': {}, 'unresolved_phase_labels': [], 'history_spans': []}
    history = {}
    occupied = set()
    for section in spec['history']:
        key, reference = section['work'], section['ref']
        require(key in expected, 'History refers to an unknown task')
        coordinates = {(reference['file'], line) for line in range(reference['start'], reference['end'] + 1)}
        require(not occupied.intersection(coordinates), 'Repeated history text must not inflate the sample')
        occupied.update(coordinates)
        history.setdefault(key, []).append(mapping.read(reference))
        source_map['history_spans'].append(section)
    overrides = {item['work']: item for item in spec['overrides']}
    require(len(overrides) == len(spec['overrides']) and set(overrides) <= seen, 'Invalid task override identity')
    redactions = {}
    for item in spec.get('history_redactions', []):
        identity = (item['work'], item['line'])
        require(item['field'] == 'summary' and item['work'] in history and identity not in redactions
                and item.get('reason'), 'Only explicit, unique history redactions are allowed')
        redactions[identity] = item
    source_map['history_redactions'] = spec.get('history_redactions', [])
    for item in inventory:
        key, cells = item['external_key'], item['source_cells']
        layout = layouts[item['source_file']]
        status = spec['status_mapping'][cells[1]]
        row = {'id': key, 'title': cells[layout['title_cell']], 'status': status,
               'acceptance': [cells[layout['acceptance_cell']]], 'next_action': '', 'blocker': None,
               'summary': '\n\n'.join(history.get(key, [])), 'depends_on': [], 'tags': [], 'evidence': []}
        references = {'table': item, 'status_legend': layout['status_legend'],
                      'missing_facts': ['next_action and blocker are unspecified unless explicitly mapped',
                                        'dependencies contain only explicitly mapped edges, not an inferred complete graph']}
        if layout.get('phase_cell') is not None:
            phase = cells[layout['phase_cell']]
            references['raw_phase'] = phase
            if phase in milestone_ids:
                row['milestone'] = phase
            else:
                row['tags'].append('source-phase:' + phase)
                source_map['unresolved_phase_labels'].append({'work': key, 'raw_phase': phase})
        if key in overrides:
            for field, value in overrides[key]['fields'].items():
                require(field in {'status', 'blocker', 'next_action', 'depends_on', 'goals'}, 'Unsupported task override field')
                row[field] = mapping.field(value)
            references['overrides'] = overrides[key]['fields']
        require(row['status'] in {'planned', 'ready', 'claimed', 'in_progress', 'blocked', 'completed', 'cancelled'},
                'Unknown normalized status')
        require(all(target in seen for target in row['depends_on']), 'Unknown dependency target')
        require(all(target in {goal['id'] for goal in document['goals']} for target in row.get('goals', [])), 'Unknown goal target')
        if any(identity[0] == key for identity in redactions):
            lines = row['summary'].splitlines()
            for (identity, number), redaction in redactions.items():
                if identity != key:
                    continue
                require(type(number) is int and 1 <= number <= len(lines), 'Invalid historical redaction line')
                require(lines[number - 1] == mapping.read(redaction['source']), 'Historical redaction differs from its original source')
                lines[number - 1] = '[withheld]'
            row['summary'] = '\n'.join(lines)
        # These are original report/document references, not newly verified evidence.
        text = row['summary'] + '\n' + row['acceptance'][0]
        locators = sorted(set(re.findall(r'`((?:docs|target|fixtures)/[^`\s]+\.(?:md|json|yaml|csv))`', text)))
        for locator in locators:
            row['evidence'].append({'locator': locator, 'summary': 'Historical source reference; original verification was not rerun by this benchmark.'})
        references['evidence_locators'] = locators
        source_map['work_items'][key] = references
        document['work_items'].append(row)
    rules = []
    for rule in spec['rules']:
        title, body = mapping.read(rule['title']), mapping.read(rule['body'])
        require(rule['scope'] in {'project', 'work_item'}, 'Unsupported rule scope')
        rules.append(f"# {title} {{#{rule['id']} severity=hard scope={rule['scope']} value={rule['value']}}}\n\n{body}\n")
    (output / 'decisions').mkdir(parents=True)
    for index, decision in enumerate(spec['decisions']):
        data = {'id': decision['id'], **{field: mapping.field(value) for field, value in decision['fields'].items()}}
        (output / 'decisions' / f'{index + 1:02}.md').write_text(
            '---\n' + yaml.safe_dump(data, allow_unicode=True, sort_keys=False) + '---\n\n# ' + data['title'] + '\n', encoding='utf-8')
    (output / 'RULES.md').write_text('\n'.join(rules), encoding='utf-8')
    (output / 'work-ledger.yaml').write_text(yaml.safe_dump(document, allow_unicode=True, sort_keys=False, width=120), encoding='utf-8')
    (output / 'project.toml').write_text('''[project]
name = "Frozen real ledger benchmark"
external_key = "real-ledger-sample-01"
authority_mode = "source_first"

[[sources]]
domain = "ledger"
role = "primary"
path = "work-ledger.yaml"
adapter = "yaml-ledger-v1"

[[sources]]
domain = "rules"
role = "primary"
path = "RULES.md"
adapter = "markdown-rules-v1"

[[sources]]
domain = "decisions"
role = "supporting"
path = "decisions"
adapter = "markdown-directory-v1"
''', encoding='utf-8')
    source_map.update(rules=spec['rules'], decisions=spec['decisions'], milestones=spec['milestones'], goals=spec['goals'])
    return document, source_map


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--intake', type=Path, required=True)
    parser.add_argument('--mapping', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    intake, mapping_path = args.intake.resolve(strict=True), args.mapping.resolve(strict=True)
    output = args.output.resolve()
    for path in [intake, mapping_path, output]:
        path.relative_to(ROOT / '.local')
    require(not output.exists(), 'Preserve old runs; choose a new preparation directory')
    before = source_state(intake)
    spec = json.loads(mapping_path.read_text(encoding='utf-8'))
    inputs = {name: digest(intake / name) for name in ['intake.json', 'work-inventory.json']}
    output.mkdir(parents=True)
    project = output / 'project'
    project.mkdir()
    document, source_map = mapped_project(intake, spec, project)
    write_json(output / 'source-map.json', source_map)
    shutil.copyfile(mapping_path, output / 'mapping-spec.json')
    contract = json.loads((HERE / 'contract.json').read_text(encoding='utf-8'))
    admission = inspect_bytes((project / 'work-ledger.yaml').read_bytes(), contract)
    require(admission['scale_eligible'], 'Canonical sample does not meet the unchanged scale contract')
    require(source_state(intake) == before and {name: digest(intake / name) for name in inputs} == inputs,
            'Original or frozen intake changed during preparation')
    counts = Counter(row['status'] for row in document['work_items'])
    report = {'kind': 'real_ledger_preparation', 'version': 1,
              'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'),
              'intake_path': str(intake), 'source_state': before, 'intake_hashes': inputs,
              'mapping_sha256': digest(mapping_path), 'source_map_sha256': digest(output / 'source-map.json'),
              'preparation_code': {path.name: digest(path) for path in [HERE / 'prepare.py', HERE / 'inspect_sample.py']},
              'history_redactions': len(spec.get('history_redactions', [])),
              'project_files': {p.relative_to(project).as_posix(): digest(p) for p in sorted(project.rglob('*')) if p.is_file()},
              'admission': admission, 'status_counts': dict(counts), 'history_sections': len(spec['history']),
              'history_tasks': len({item['work'] for item in spec['history']}),
              'evidence_references': sum(len(row['evidence']) for row in document['work_items']),
              'distinct_evidence_locators': len({ev['locator'] for row in document['work_items'] for ev in row['evidence']}),
              'milestones': len(document['milestones']), 'decisions': len(spec['decisions']),
              'unresolved_phase_labels': source_map['unresolved_phase_labels'],
              'originals_unchanged': True, 'runtime_benchmark_executed': False, 'benchmark_completed': False,
              'retention': 'local_only'}
    write_json(output / 'preparation.json', report)
    print(json.dumps({k: report[k] for k in ['status_counts', 'history_sections', 'evidence_references', 'milestones', 'decisions', 'benchmark_completed']}))


if __name__ == '__main__':
    main()
