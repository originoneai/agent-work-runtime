"""Small parser checks only; these fixtures are not real large-project evidence."""
import json
import unittest

from inspect_sample import CONTRACT, inspect_bytes


class AdmissionTests(unittest.TestCase):
    def inspect(self, source):
        return inspect_bytes(source.encode(), json.loads(CONTRACT.read_text()))

    def test_status_partitions_do_not_promote_unknown_or_complete_benchmark(self):
        result = self.inspect('''work_items:
  - {id: active, status: in_progress}
  - {id: old, status: completed}
  - {id: cancelled, status: cancelled}
  - {id: unfamiliar, status: archived}
''')
        self.assertEqual((result['unique_work_items'], result['current_work_items'],
                          result['historical_work_items'], result['unknown_status_items']), (4, 1, 2, 1))
        self.assertFalse(result['scale_eligible'])
        self.assertFalse(result['benchmark_completed'])
        self.assertFalse(result['runtime_benchmark_executed'])
        self.assertEqual(result['e4_credit'], 0)

    def test_duplicate_or_conflicting_identities_are_rejected(self):
        cases = [
            'work_items: [{id: one, status: ready}, {id: one, status: completed}]',
            'work_items: {one: {id: two, status: ready}}',
            'work_items: {one: {status: ready}, one: {status: completed}}',
            'work_items: [{external_key: "", id: one, status: ready}]',
            'work_items: [{external_key: false, id: one, status: ready}]',
            'work_items: [{id: one, status: ready, status: completed}]',
        ]
        for source in cases:
            with self.subTest(case=cases.index(source)), self.assertRaises(ValueError):
                self.inspect(source)

    def test_keyed_ledger_counts_unique_entities(self):
        result = self.inspect('''work_items:
  one: {status: ready}
  two: {external_key: two, status: completed}
milestones:
  first: {title: First milestone}
  second: {id: second, title: Second milestone}
''')
        self.assertEqual(result['unique_work_items'], 2)
        self.assertEqual(result['inline_milestones'], 2)
        self.assertFalse(result['real_provenance_verified'])

    def test_oversized_input_fails_before_yaml_parsing(self):
        contract = json.loads(CONTRACT.read_text())
        contract['scale']['maximum_ledger_bytes'] = 4
        with self.assertRaisesRegex(ValueError, 'cap'):
            inspect_bytes(b'not YAML at all', contract)


if __name__ == '__main__':
    unittest.main()
