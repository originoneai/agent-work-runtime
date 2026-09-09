#!/usr/bin/env python3
"""Reproduce the exact public synthetic log for this retained attempt.

The generator is copied from tests/fixtures/business/prepare.py. This script
creates only a caller-selected new log file; it never creates a project,
restricted material, Git repository, AWR runtime or acceptance result.
"""
import argparse
import hashlib
import json
from pathlib import Path

RUN_ID = 'awr-live-20260909-007-r2'
MINIMUM_BYTES = 3 * 1024 * 1024
EXPECTED_BYTES = 3145837
EXPECTED_SHA256 = '058e7a7ed4c472d8e2e868e9e6aa57216944fe513bf3cb303d5bb31d73f35cf1'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('Output already exists; choose a new file.')
    payload = bytearray()
    index = 0
    while len(payload) <= MINIMUM_BYTES:
        batch = index % 47
        day = 1 if batch < 12 else 2 if batch < 22 else 3 if batch < 36 else 4
        row = {'event': RUN_ID + '-' + str(index), 'day': day,
               'batch': f'public-batch-{batch + 1:02}',
               'stage': ['queued', 'parsed', 'recorded'][index % 3],
               'result': 'retry' if index == 17 else 'delayed' if index in (45, 46) else 'ok',
               'elapsed_ms': 20 + (index % 31), 'note': 'public synthetic operations record'}
        payload.extend((json.dumps(row, separators=(',', ':')) + '\n').encode('utf-8'))
        index += 1
    digest = hashlib.sha256(payload).hexdigest()
    assert len(payload) == EXPECTED_BYTES and digest == EXPECTED_SHA256
    with args.output.open('xb') as stream:
        stream.write(payload)
    print(json.dumps({'path': str(args.output), 'run_id': RUN_ID,
                      'bytes': len(payload), 'records': index, 'sha256': digest}))

if __name__ == '__main__':
    main()
