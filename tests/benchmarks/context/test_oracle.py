"""Loss detection tests for the measurement oracle, not business acceptance."""
import copy
import unittest

from oracle import assess


def fixture():
    source = {'id': 'W', 'status': 'blocked', 'next_action': 'Revise the handover.',
              'acceptance': ['Deliver an editable handover.', 'Include the receipt.'],
              'blocker': 'Waiting for the owner reply.'}
    dependency = {'id': 'D', 'status': 'blocked', 'next_action': 'Review the attached receipt.',
                  'blocker': 'Receipt awaiting approval.'}
    oracle = {'work': source, 'rules': [{'key': 'facts', 'text': 'Facts\n\nPreserve every approved term.'}],
              'unresolved_dependencies': [dependency]}
    work = {k: v for k, v in source.items() if k != 'id'}
    work.update(id='01M20Z76MSAKBNFD120VXK75Z3', external_key='W', raw_status='blocked')
    l0 = ('W 01M20Z76MSAKBNFD120VXK75Z3 Status: blocked\nRevise the handover.\n'
          'Waiting for the owner reply.\nFacts\n\nPreserve every approved term.')
    l1 = ('W 01M20Z76MSAKBNFD120VXK75Z3 Status: "blocked"\nBlocker present: true\n'
          'Revise the handover.\nWaiting for the owner reply.\nDeliver an editable handover.\n'
          'Include the receipt.\nFacts\n\nPreserve every approved term.\n'
          'D Status: "blocked" (raw "blocked")\nReview the attached receipt.\nReceipt awaiting approval.')
    bootstrap = {'rendered_context': l0, 'context': {'work': copy.deepcopy(work), 'critical_rules': [
        {'external_key': 'RULES.md#facts', 'text': oracle['rules'][0]['text']}]}}
    compiled = {'work_context': {'identity': {'work_item_key': 'W', 'work_item_id': work['id']},
        'rendered_context': l1, 'selected_chunks': [{'key': 'required:D', 'required': True, 'section': 'dependencies'}]},
        'completeness': {'unresolved_required_dependencies': ['D']}}
    return oracle, work, bootstrap, compiled


class OracleTests(unittest.TestCase):
    def test_complete_source_facts_and_nonempty_dependency_pass(self):
        result = assess(*fixture())
        self.assertEqual(result['recall'], 1)
        self.assertEqual(len(result['categories']), 7)

    def test_each_missing_verbatim_fact_fails_its_category(self):
        losses = [('work_item_id', '01M20Z76MSAKBNFD120VXK75Z3'), ('status', 'Status: "blocked"'),
                  ('next_action', 'Revise the handover.'), ('acceptance', 'Include the receipt.'),
                  ('blocker', 'Waiting for the owner reply.'), ('applicable_hard_rules', 'Preserve every approved term.'),
                  ('unresolved_required_dependencies', 'Review the attached receipt.')]
        for category, text in losses:
            with self.subTest(category=category):
                oracle, work, bootstrap, compiled = fixture()
                compiled['work_context']['rendered_context'] = compiled['work_context']['rendered_context'].replace(text, '')
                result = assess(oracle, work, bootstrap, compiled)
                self.assertLess(result['recall'], 1)
                self.assertIn(False, result['categories'][category].values())

    def test_same_words_do_not_hide_wrong_identity_or_nonmandatory_dependency(self):
        oracle, work, bootstrap, compiled = fixture()
        compiled['work_context']['identity']['work_item_id'] = 'different'
        compiled['work_context']['selected_chunks'][0]['required'] = False
        result = assess(oracle, work, bootstrap, compiled)
        self.assertFalse(result['categories']['work_item_id']['runtime_identity'])
        self.assertFalse(result['categories']['unresolved_required_dependencies']['required_identity_0'])

    def test_no_pending_dependency_is_checked_as_an_exact_empty_set(self):
        oracle, work, bootstrap, compiled = fixture()
        oracle['unresolved_dependencies'] = []
        result = assess(oracle, work, bootstrap, compiled)
        self.assertFalse(result['categories']['unresolved_required_dependencies']['exact_set'])
        compiled['completeness']['unresolved_required_dependencies'] = []
        self.assertEqual(assess(oracle, work, bootstrap, compiled)['recall'], 1)


if __name__ == '__main__':
    unittest.main()
