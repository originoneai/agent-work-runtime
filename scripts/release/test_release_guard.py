#!/usr/bin/env python3
"""Synthetic publication and source-boundary regressions; no registry writes."""
import copy
import json
import unittest

from check_release import MEMBERS, ROOT, validate_identity, validate_scope


class ReleaseGuardTests(unittest.TestCase):
    def test_specialist_contract_versions_and_counts_match_regression(self):
        regression = json.loads((ROOT / "tests/regression/contract.json").read_text())
        for gate in regression["gates"]:
            if gate["kind"] != "contract":
                continue
            with self.subTest(gate=gate["id"]):
                child = json.loads((ROOT / gate["contract"]).read_text())
                self.assertEqual(gate["version"], child["version"])
                conditions = child.get("cases", child.get("conditions", []))
                self.assertEqual(gate["target_conditions"], len(conditions))
                self.assertEqual(len(conditions), len({item["id"] for item in conditions}))

    def test_publish_requires_exact_manual_identity(self):
        sha = "a" * 40
        env = dict(RELEASE_PUBLISH="true", GITHUB_EVENT_NAME="workflow_dispatch",
                   GITHUB_REF="refs/heads/release/0.5.1", EXPECTED_RELEASE_VERSION="0.5.1",
                   EXPECTED_SOURCE_SHA=sha, GITHUB_SHA=sha)
        validate_identity(env, sha, "0.5.1")
        for key, value in (("EXPECTED_SOURCE_SHA", ""), ("EXPECTED_SOURCE_SHA", "a" * 7),
                           ("EXPECTED_SOURCE_SHA", "b" * 40), ("GITHUB_SHA", "b" * 40),
                           ("GITHUB_REF", "refs/heads/main"), ("GITHUB_EVENT_NAME", "pull_request"),
                           ("EXPECTED_RELEASE_VERSION", "0.4.0")):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                validate_identity(dict(env, **{key: value}), sha, "0.5.1")

    def test_verify_only_does_not_need_publication_identity(self):
        validate_identity({"GITHUB_EVENT_NAME": "pull_request"}, "a" * 40, "0.5.1")

    def test_scope_retains_workspace_and_rejects_team(self):
        manifest = {"workspace": {"members": [f"crates/{n}" for n in sorted(MEMBERS)],
                                  "package": {"version": "0.5.1"}}}
        lock = {"package": [{"name": n} for n in MEMBERS]}
        validate_scope(manifest, lock, [])
        for name in ("awr-team", "sqlx-core", "tokio-postgres"):
            bad = copy.deepcopy(lock)
            bad["package"].append({"name": name, "source": "registry+synthetic"})
            with self.subTest(name=name), self.assertRaises(ValueError):
                validate_scope(manifest, bad, [])
        with self.assertRaises(ValueError):
            validate_scope(manifest, lock, ["crates/awr-team/src/lib.rs"])
        bad = copy.deepcopy(manifest)
        bad["workspace"]["members"].remove("crates/awr-workspace")
        with self.assertRaises(ValueError):
            validate_scope(bad, lock, [])


if __name__ == "__main__":
    unittest.main()
