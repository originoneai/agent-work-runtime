#!/usr/bin/env python3
"""Read-only structural admission for a canonical ledger; no benchmark pass is inferred."""
import argparse
from collections import Counter
from datetime import datetime
import hashlib
import json
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / 'tests/long-ledger/contract.json'


def require(condition, message):
    if not condition:
        raise ValueError(message)


class UniqueLoader(yaml.SafeLoader):
    pass


def unique_mapping(loader, node, deep=False):
    pairs = loader.construct_pairs(node, deep=deep)
    result = {}
    for key, value in pairs:
        require(isinstance(key, (str, int, float, bool)) and key not in result,
                'Duplicate or unsupported YAML mapping key; no source text is echoed')
        result[key] = value
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)


def entries(value, label):
    if value is None:
        return []
    require(isinstance(value, (list, dict)), label + ' must be a list or keyed mapping')
    rows = list(value.items()) if isinstance(value, dict) else [(None, v) for v in value]
    seen, result = set(), []
    for fallback, row in rows:
        require(isinstance(row, dict), label + ' contains a non-mapping entry')
        require(all(row.get(field) is None or isinstance(row[field], str)
                    for field in ['external_key', 'id']), label + ' identity fields must be strings')
        key = row.get('external_key')
        if key is None:
            key = row.get('id')
        if key is None:
            key = fallback
        require(isinstance(key, str) and bool(key.strip()), label + ' needs stable string keys')
        require(fallback is None or fallback == key, label + ' map key differs from the entry key')
        require(key not in seen, label + ' repeats a work/entity key')
        seen.add(key)
        result.append(row)
    return result


def inspect_bytes(body, contract):
    scale = contract['scale']
    require(len(body) <= scale['maximum_ledger_bytes'], 'Ledger exceeds the current YAML source-reader cap')
    try:
        document = yaml.load(body.decode('utf-8'), Loader=UniqueLoader)
    except (UnicodeError, yaml.YAMLError):
        raise ValueError('Invalid UTF-8/YAML ledger; inspect the source locally') from None
    require(isinstance(document, dict) and 'work_items' in document, 'A canonical work_items source is required')
    work = entries(document['work_items'], 'work_items')
    require(bool(work), 'No work items were found')
    statuses = Counter()
    for row in work:
        status = row.get('status')
        require(isinstance(status, str), 'Every counted work item needs an explicit string status')
        statuses[status] += 1
    current = sum(statuses[s] for s in scale['current_statuses'])
    historical = sum(statuses[s] for s in scale['historical_statuses'])
    unknown = len(work) - current - historical
    checks = {
        'current_work_minimum': current >= scale['minimum_current_work_items'],
        'historical_work_minimum': historical >= scale['minimum_historical_work_items'],
        'ledger_byte_minimum': len(body) >= scale['minimum_ledger_bytes'],
        'all_statuses_classified': unknown == 0,
    }
    return {'source_bytes': len(body), 'unique_work_items': len(work), 'current_work_items': current,
            'historical_work_items': historical, 'unknown_status_items': unknown,
            'inline_milestones': len(entries(document.get('milestones'), 'milestones')),
            'scale_checks': checks, 'scale_eligible': all(checks.values()),
            'real_provenance_verified': False, 'domain_richness_verified': False,
            'runtime_benchmark_executed': False, 'benchmark_completed': False,
            'metric_credit': 0, 'e4_credit': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ledger', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = args.ledger.resolve(strict=True)
    output = args.output.resolve()
    output.relative_to(ROOT / '.local')
    require(source.is_file(), 'Select one authoritative ledger file')
    require(not output.exists(), 'Use a new local report path')
    contract = json.loads(CONTRACT.read_text(encoding='utf-8'))
    with source.open('rb') as stream:
        body = stream.read(contract['scale']['maximum_ledger_bytes'] + 1)
    report = inspect_bytes(body, contract)
    fingerprint = hashlib.sha256(body).hexdigest()
    with source.open('rb') as stream:
        checked_body = stream.read(contract['scale']['maximum_ledger_bytes'] + 1)
    require(checked_body == body, 'Source changed during inspection')
    report.update(contract_id=contract['contract_id'], contract_version=contract['version'],
                  contract_sha256=hashlib.sha256(CONTRACT.read_bytes()).hexdigest(),
                  inspected_at=datetime.now().astimezone().isoformat(timespec='seconds'),
                  source_path=str(source), source_sha256=fingerprint, source_unchanged=True,
                  retention='local_only', required_runtime_gates=contract['required_gates'])
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['unique_work_items', 'current_work_items', 'historical_work_items',
                                           'source_bytes', 'scale_eligible', 'benchmark_completed', 'e4_credit']}))
    # Ineligibility is a successful assessment, not a successful benchmark.
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except ValueError as error:
        raise SystemExit(str(error)) from None
