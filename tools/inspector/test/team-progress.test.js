'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { mapObservation, pullRequestReference, createGithubObserver } = require('../team-progress');

test('observation separates a claimant, responsible person, expired claim and unverified report', () => {
  const mapped = mapObservation({
    runtime: { state: 'claimed' }, session: { id: 's', actor_name: 'Member', client_id: 'client' },
    claim: { state: 'active', lease_live: false }, model: null, usage: null,
    checkpoint: { contract_matches_current: false, next_action: 'An old instruction' },
    execution: { state: 'running', receipt_details_available: false },
  });
  assert.equal(mapped.claimant, 'Member');
  assert.equal(mapped.owner_person, null);
  assert.equal(mapped.agent, null);
  assert.equal(mapped.client_id, 'client');
  assert.equal(mapped.attention, 'lease_expired');
  assert.equal(mapped.next_step, null);
  assert.equal(mapped.execution.report, null);
  assert.equal(mapped.model, null);
  assert.equal(mapped.usage, null);
  assert.equal(mapObservation({ runtime: { state: 'completed' }, claim: { state: 'active', lease_live: false } }).attention, null);
});

test('only exact GitHub references in authorized current checkpoints or registered deliveries are used', () => {
  const checkpoint = { contract_matches_current: true, next_action: 'Review example/repo#12.', open_loops: [] };
  assert.deepEqual(pullRequestReference({ checkpoint }), {
    owner: 'example', repo: 'repo', number: 12, url: 'https://github.com/example/repo/pull/12',
    registration: 'checkpoint_mention', expected_head: null,
  });
  assert.equal(pullRequestReference({ checkpoint: { ...checkpoint, contract_matches_current: false } }), null);
  for (const text of ['https://github.com.evil.test/a/b/pull/1', 'http://127.0.0.1/pull/1', 'javascript:alert(1)', 'https://github.com/../../pull/1'])
    assert.equal(pullRequestReference({ checkpoint: { ...checkpoint, next_action: text } }), null);
});

test('GitHub observation checks the current SHA, combines checks and statuses, and caches concurrent reads', async () => {
  const ref = pullRequestReference({ checkpoint: { contract_matches_current: true, next_action: 'example/repo#12' } });
  let calls = 0;
  const observe = createGithubObserver(async (url, options) => {
    calls++;
    assert.equal(options.redirect, 'error');
    assert.equal(options.headers.authorization, undefined);
    const json = url.endsWith('/pulls/12') ? { html_url: ref.url, head: { sha: 'a'.repeat(40) }, state: 'open' }
      : url.includes('/check-runs?') ? { total_count: 1, check_runs: [{ status: 'completed', conclusion: 'success' }] }
        : { total_count: 1, statuses: [{ state: 'failure' }] };
    return { ok: true, json: async () => json };
  }, () => 1234);
  const [a, b] = await Promise.all([observe(ref), observe(ref)]);
  assert.equal(calls, 3);
  assert.deepEqual(a, b);
  assert.equal(a.ci, 'failed');
  assert.equal(a.head_sha, 'a'.repeat(40));
  assert.equal(a.observed_at_ms, 1234);
  assert.equal(a.source, 'github_public_api');
});

test('missing, truncated, pending and unavailable GitHub checks never become passed', async () => {
  const ref = { url: 'https://github.com/example/repo/pull/1', owner: 'example', repo: 'repo', number: 1 };
  for (const [checks, statuses, expected] of [
    [{ total_count: 0, check_runs: [] }, { total_count: 0, statuses: [] }, 'not_reported'],
    [{ total_count: 101, check_runs: [] }, { total_count: 0, statuses: [] }, 'incomplete'],
    [{ total_count: 1, check_runs: [{ status: 'in_progress' }] }, { total_count: 0, statuses: [] }, 'pending'],
    [{ total_count: 1, check_runs: [{ status: 'completed', conclusion: 'success' }] }, { total_count: 0, statuses: [] }, 'passed'],
  ]) {
    const observe = createGithubObserver(async url => ({ ok: true, json: async () => url.endsWith('/pulls/1')
      ? { html_url: ref.url, head: { sha: 'b'.repeat(40) }, state: 'open' }
      : url.includes('/check-runs?') ? checks : statuses }));
    assert.equal((await observe(ref)).ci, expected);
  }
  assert.equal((await createGithubObserver(async () => { throw new Error('unavailable'); })(ref)).unavailable, true);
});
