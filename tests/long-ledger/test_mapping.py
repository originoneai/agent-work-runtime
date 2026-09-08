import copy
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from prepare import Mapping, digest_bytes, mapped_project, write_json


class MappingTests(unittest.TestCase):
    def setUp(self):
        self.temp = TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'originals').mkdir()
        self.lines = ['# Ledger', '[ ] means planned; [x] means completed.',
                      '| CURRENT | [ ] | M1+M2 | Prepare the guide | Preserve source facts. |',
                      '| OLD | [x] | M1 | Old stockroom guide | Retain the archived notes. |',
                      '## Receipt', 'External supplier confirmation is missing.',
                      'Obtain the supplier confirmation.', 'Archived source receipt.']
        self.write_source()
        self.spec = {'version': 1, 'tables': [{'file': 'ledger.md', 'key_pattern': 'CURRENT|OLD', 'columns': 5,
                                              'title_cell': 3, 'acceptance_cell': 4, 'phase_cell': 2,
                                              'status_legend': self.ref(2)}],
                     'status_mapping': {'[ ]': 'planned', '[x]': 'completed'},
                     'milestones': [], 'goals': [], 'rules': [], 'decisions': [],
                     'history': [{'work': 'OLD', 'ref': self.ref(8)}],
                     'overrides': [{'work': 'CURRENT', 'fields': {
                         'status': {'value': 'blocked', 'basis': [self.ref(6)],
                                    'interpretation': 'Explicit missing external confirmation.'},
                         'blocker': {'ref': self.ref(6)}, 'next_action': {'ref': self.ref(7)}}}]}

    def write_source(self):
        (self.root / 'originals/ledger.md').write_text('\n'.join(self.lines) + '\n')
        write_json(self.root / 'intake.json', {'sources': [{'frozen_path': 'originals/ledger.md'}]})
        inventory = []
        for number in [3, 4]:
            cells = [cell.strip() for cell in self.lines[number - 1][1:-1].split('|')]
            inventory.append({'external_key': cells[0], 'source_file': 'ledger.md', 'source_line': number,
                              'source_cells': cells, 'source_line_sha256': digest_bytes(self.lines[number - 1].encode())})
        write_json(self.root / 'work-inventory.json', inventory)

    def ref(self, start, end=None):
        end = start if end is None else end
        return {'file': 'ledger.md', 'start': start, 'end': end,
                'sha256': digest_bytes('\n'.join(self.lines[start - 1:end]).encode())}

    def map(self, spec=None):
        target = self.root / 'project'
        target.mkdir()
        return mapped_project(self.root, spec or self.spec, target)

    def test_preserves_status_exception_history_and_unresolved_phase(self):
        document, provenance = self.map()
        current, old = document['work_items']
        self.assertEqual((current['status'], current['blocker'], current['next_action']),
                         ('blocked', self.lines[5], self.lines[6]))
        self.assertEqual(old['summary'], self.lines[7])
        self.assertEqual(old['next_action'], '')
        self.assertEqual(provenance['work_items']['CURRENT']['table']['source_cells'][1], '[ ]')
        self.assertNotIn('milestone', current)
        self.assertEqual(provenance['unresolved_phase_labels'][0]['raw_phase'], 'M1+M2')
        self.assertEqual((self.root / 'originals/ledger.md').read_text(), '\n'.join(self.lines) + '\n')

    def test_changed_source_span_is_rejected(self):
        mapping = Mapping(self.root, self.spec)
        bad = self.ref(6)
        bad['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'fingerprint'):
            mapping.read(bad)
        with self.assertRaisesRegex(ValueError, 'evidence'):
            mapping.field({'value': 'ready'})

    def test_unknown_status_is_not_silently_dropped(self):
        self.lines[2] = self.lines[2].replace('[ ]', '[?]')
        self.write_source()
        with self.assertRaisesRegex(ValueError, 'Unknown source status'):
            self.map()

    def test_duplicate_task_or_repeated_history_is_rejected(self):
        spec = copy.deepcopy(self.spec)
        spec['history'].append(copy.deepcopy(spec['history'][0]))
        with self.assertRaisesRegex(ValueError, 'Repeated history'):
            self.map(spec)
        inventory = json.loads((self.root / 'work-inventory.json').read_text())
        inventory.append(inventory[0])
        write_json(self.root / 'work-inventory.json', inventory)
        with self.assertRaisesRegex(ValueError, 'Duplicate inventory'):
            mapped_project(self.root, self.spec, self.root / 'unused')

    def test_inventory_and_original_row_must_match(self):
        self.lines[2] = self.lines[2].replace('Prepare the guide', 'Prepare a different guide')
        (self.root / 'originals/ledger.md').write_text('\n'.join(self.lines) + '\n')
        with self.assertRaisesRegex(ValueError, 'disagree'):
            self.map()

    def test_reviewed_redaction_is_bound_to_history_and_keeps_source(self):
        spec = copy.deepcopy(self.spec)
        spec['history_redactions'] = [{'work': 'OLD', 'field': 'summary', 'line': 1,
                                      'source': self.ref(8), 'reason': 'Reviewed private historical data.'}]
        document, provenance = self.map(spec)
        self.assertEqual(document['work_items'][1]['summary'], '[withheld]')
        self.assertEqual(provenance['history_redactions'], spec['history_redactions'])
        self.assertIn(self.lines[7], (self.root / 'originals/ledger.md').read_text())
        spec['history_redactions'][0]['field'] = 'acceptance'
        with self.assertRaisesRegex(ValueError, 'history redactions'):
            mapped_project(self.root, spec, self.root / 'unused')


if __name__ == '__main__':
    unittest.main()
