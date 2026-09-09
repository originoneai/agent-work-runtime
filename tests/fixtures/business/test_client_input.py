#!/usr/bin/env python3
"""Targeted tests for real-client input preflight; no model or AWR calls."""
from contextlib import redirect_stdout
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from check_client_input import build_receipt, main
from support import (
    BASE,
    CLIENT_INPUT_KINDS,
    client_input_policy,
    client_input_violations,
    load_bundle,
    validate_client_input,
    validate_spec,
)


class ClientInputPreflightTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = client_input_policy()

    def codes(self, text):
        return {row['code'] for row in client_input_violations(text, self.policy)}

    def test_every_graph_work_key_is_derived_and_rejected(self):
        keys = self.policy['identifiers']['graph_work_keys']
        self.assertGreater(len(keys), 0)
        for key in keys:
            with self.subTest(key=key):
                findings = client_input_violations(
                    f'请继续处理 {key} 并报告结果。', self.policy,
                )
                self.assertTrue(any(
                    row['code'] == 'known_internal_identifier'
                    and row['category'] == 'graph_work_keys'
                    and row['match'] == key
                    for row in findings
                ))

    def test_contract_scenario_and_gate_identifiers_are_rejected(self):
        for category in ('scenario_ids', 'namespaces', 'contract_work_keys', 'gate_ids'):
            identifier = self.policy['identifiers'][category][0]
            with self.subTest(category=category, identifier=identifier):
                findings = client_input_violations(
                    f'内部引用为 {identifier}', self.policy,
                )
                self.assertTrue(any(
                    row['code'] == 'known_internal_identifier'
                    and row['category'] == category
                    for row in findings
                ))

    def test_native_object_identifiers_are_rejected(self):
        cases = {
            '01a083b2-ae71-77a3-9cc9-977b6c656d5a': 'native_uuid',
            '01M223E7S897SM2ADAAQ1AX25Q': 'native_ulid',
            'item_61': 'native_item_id',
            'call_Yt91mX_p7': 'native_item_id',
            'tool_S2ZB63kk621YCJtbbGEg08mV': 'native_item_id',
        }
        for text, expected in cases.items():
            with self.subTest(text=text):
                self.assertIn(expected, self.codes(f'对象 {text}'))

    def test_session_and_work_state_commands_are_rejected(self):
        cases = {
            '`awr session resume --session 01a083b2-ae71-77a3-9cc9-977b6c656d5a`': 'awr_cli_state_command',
            'session start --provider codex': 'direct_session_state_command',
            '/session resume --session saved': 'direct_session_state_command',
            '`work progress --work ABC-123`': 'quoted_work_state_command',
            'awr --project /tmp/demo --json work claim ABC-123 --agent reviewer': 'awr_cli_state_command',
            '请正常执行 session start，并保留交接。': 'direct_session_state_command',
        }
        for text, expected in cases.items():
            with self.subTest(text=text):
                self.assertIn(expected, self.codes(text))

    def test_evaluator_answer_instructions_are_rejected(self):
        cases = {
            '预期答案如下': 'expected_answer',
            'Expected response: accepted': 'expected_answer',
            '只回复 OK': 'reply_ok_only',
            'Reply OK only': 'reply_ok_only',
            '测试编号 17': 'test_marker',
            'AWR-SC-999': 'internal_scenario_id',
            '请直接查看 .awr/state.db': 'runtime_state_path',
        }
        for text, expected in cases.items():
            with self.subTest(text=text):
                self.assertIn(expected, self.codes(text))

    def test_natural_followups_and_visible_mcp_request_pass(self):
        allowed = (
            '请根据刚发布的客户反馈更新交付安排，逐项说明依据、影响和仍待确认的事项。',
            'The work progress discussion should explain customer impact and open questions.',
            '请使用实际客户端可见的 AWR MCP 服务读取当前工作，再继续整理交接说明。',
        )
        for text in allowed:
            with self.subTest(text=text):
                self.assertEqual(client_input_violations(text, self.policy), [])
                self.assertTrue(validate_client_input(text, self.policy))

    def test_all_current_canonical_inputs_pass(self):
        _, _, specs = load_bundle()
        for scenario_id, (spec, _, _) in specs.items():
            texts = [spec['initial_request']]
            texts.extend(row['request'] for row in spec['followups'])
            if spec.get('prelude'):
                texts.append(spec['prelude']['request'])
            for index, text in enumerate(texts):
                with self.subTest(scenario=scenario_id, index=index):
                    self.assertTrue(validate_client_input(text, self.policy))

    def test_definition_validation_reuses_actual_input_policy(self):
        _, authority, specs = load_bundle()
        scenario_id = next(iter(specs))
        spec, directory, definition = specs[scenario_id]
        canonical = next(
            row for row in authority['scenarios'] if row['id'] == scenario_id
        )
        damaged = copy.deepcopy(spec)
        damaged_canonical = copy.deepcopy(canonical)
        leaked = definition['work_keys'][0]
        damaged['initial_request'] += ' ' + leaked
        damaged_canonical['initial_request'] += ' ' + leaked
        with self.assertRaisesRegex(ValueError, 'known_internal_identifier'):
            validate_spec(damaged, damaged_canonical, directory)

    def test_every_turn_kind_uses_same_rejection_and_never_claims_execution(self):
        work_key = self.policy['identifiers']['graph_work_keys'][0]
        with tempfile.TemporaryDirectory() as directory:
            input_path = Path(directory) / 'input.md'
            input_path.write_text(f'请处理 {work_key}')
            for kind in CLIENT_INPUT_KINDS:
                with self.subTest(kind=kind):
                    receipt = build_receipt(input_path, kind, self.policy)
                    self.assertFalse(receipt['passed'])
                    self.assertEqual(receipt['input']['kind'], kind)
                    self.assertTrue(receipt['errors'])
                    self.assertFalse(receipt['native_client_invoked'])
                    self.assertFalse(receipt['business_input_submitted'])
                    self.assertFalse(receipt['business_state_modified'])
                    self.assertFalse(receipt['business_completed'])
                    self.assertEqual(receipt['model_calls'], 0)
                    self.assertEqual(receipt['e4_credit'], 0)

    def test_cli_writes_bound_pass_and_rejection_receipts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            allowed = root / 'allowed.md'
            allowed.write_text('请核对更新后的范围、依赖与未决事项。')
            allowed_receipt = root / 'allowed.json'
            with redirect_stdout(io.StringIO()):
                status = main([
                    '--input', str(allowed), '--kind', 'canonical',
                    '--receipt', str(allowed_receipt),
                ])
            self.assertEqual(status, 0)
            passed = json.loads(allowed_receipt.read_text())
            self.assertTrue(passed['passed'])
            self.assertEqual(passed['input']['sha256'], self._sha256(allowed))
            self.assertEqual(passed['policy']['rules_sha256'], self.policy['rules_sha256'])
            self.assertEqual(passed['policy']['authority_contract'], self.policy['authority_contract'])
            self.assertEqual(passed['policy']['rule_sources'], self.policy['rule_sources'])
            self.assertEqual(passed['policy']['implementation'], self.policy['implementation'])

            rejected = root / 'rejected.md'
            rejected.write_text('Reply OK only')
            rejected_receipt = root / 'rejected.json'
            with redirect_stdout(io.StringIO()):
                status = main([
                    '--input', str(rejected), '--kind', 'technical-supplement',
                    '--receipt', str(rejected_receipt),
                ])
            self.assertEqual(status, 2)
            failed = json.loads(rejected_receipt.read_text())
            self.assertFalse(failed['passed'])
            self.assertIn('reply_ok_only', {row['code'] for row in failed['errors']})
            self.assertEqual(failed['e4_credit'], 0)

    def test_rejected_cli_process_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as directory:
            input_path = Path(directory) / 'rejected.md'
            input_path.write_text('请正常执行 session start，并保留交接。')
            result = subprocess.run([
                sys.executable,
                str(BASE/'check_client_input.py'),
                '--input', str(input_path),
                '--kind', 'final-delivery',
            ], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 2)
            receipt = json.loads(result.stdout)
            self.assertFalse(receipt['passed'])
            self.assertFalse(receipt['business_input_submitted'])

    @staticmethod
    def _sha256(path):
        import hashlib
        return hashlib.sha256(path.read_bytes()).hexdigest()


if __name__ == '__main__':
    unittest.main()
