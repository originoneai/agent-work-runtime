#!/usr/bin/env python3
"""Preflight one actual-client input file without invoking a client or AWR."""
import argparse
from datetime import datetime
import json
from pathlib import Path

from support import (
    CLIENT_INPUT_KINDS,
    ROOT,
    client_input_policy,
    client_input_violations,
    digest,
    require,
)


def build_receipt(input_path, kind, policy=None):
    """Return a receipt for one input; this function has no runtime side effects."""
    require(kind in CLIENT_INPUT_KINDS, 'Unknown actual-client input kind')
    policy = policy or client_input_policy()
    input_path = input_path.resolve(strict=True)
    require(input_path.is_file(), 'Client input must be a regular file')
    raw = input_path.read_bytes()
    errors = []
    try:
        text = raw.decode('utf-8')
    except UnicodeDecodeError as error:
        errors.append({
            'code': 'invalid_utf8',
            'detail': 'Actual client input must be UTF-8 text.',
            'offset': error.start,
        })
    else:
        errors.extend(client_input_violations(text, policy))

    try:
        displayed_path = str(input_path.relative_to(ROOT))
    except ValueError:
        displayed_path = str(input_path)
    return {
        'receipt_version': '1.0.0',
        'checked_at': datetime.now().astimezone().isoformat(timespec='seconds'),
        'input': {
            'path': displayed_path,
            'kind': kind,
            'bytes': len(raw),
            'sha256': digest(input_path),
        },
        'policy': {
            'id': policy['policy_id'],
            'version': policy['policy_version'],
            'rules_sha256': policy['rules_sha256'],
            'fixture_contract': policy['fixture_contract'],
            'authority_contract': policy['authority_contract'],
            'rule_sources': policy['rule_sources'],
            'implementation': policy['implementation'],
            'work_graphs': policy['work_graphs'],
        },
        'passed': not errors,
        'errors': errors,
        'native_client_invoked': False,
        'model_calls': 0,
        'business_input_submitted': False,
        'business_state_modified': False,
        'business_completed': False,
        'e4_credit': 0,
        'limitation': (
            'This deterministic preflight does not establish naturalness or exclude '
            'all answer pollution; independent review remains required.'
        ),
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True,
                        help='UTF-8 file containing exactly the text to submit')
    parser.add_argument('--kind', choices=CLIENT_INPUT_KINDS, required=True,
                        help='Business turn category; every category uses the same policy')
    parser.add_argument('--receipt', type=Path,
                        help='Optional new JSON receipt path')
    args = parser.parse_args(argv)

    receipt = build_receipt(args.input, args.kind)
    rendered = json.dumps(receipt, ensure_ascii=False, indent=2) + '\n'
    if args.receipt:
        require(not args.receipt.exists(), 'Refusing to overwrite an input preflight receipt')
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        args.receipt.write_text(rendered)
    print(rendered, end='')
    return 0 if receipt['passed'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
