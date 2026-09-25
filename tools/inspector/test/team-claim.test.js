'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createClaimFlow } = require('../public/team-claim');
const scope = { work: 'TASK', stream: 'STREAM', contract: 'hash' };
function fixture() {
  let seq = 0, revision = 1;
  const receipts = new Map(), writes = [], reads = [], saves = [];
  const lease = { claim_id: 'claim', session_id: 'session', owned_by_client: true, lease_live: true };
  const f = { lease, writes, reads, saves, receipts, beforeWrite: null, afterWrite: null, current: true };
  f.query = async q => {
    reads.push(q);
    if (q.op === 'work.prepare') return { project_revision: String(revision), authority_version: '1', coordinator_epoch: 'epoch',
      workstream_id: 'STREAM', data: { work_id: 'TASK', ownership_version: '1', contract_hash: 'hash', context_complete: true } };
    if (q.op === 'command.inspect') return { data: { receipt: receipts.get(q.request_id) || null } };
    if (q.op === 'session.inspect') return { data: { items: [{ session_id: 'session', state: 'active', session_version: '3' }] } };
    if (q.op === 'claim.inspect') return { data: lease };
    throw Error('unexpected query');
  };
  f.command = async c => {
    if (f.beforeWrite) await f.beforeWrite(c);
    writes.push(structuredClone(c)); revision++;
    const receipt = { op: c.op, request_id: c.request_id,
      data: c.op === 'session.start' ? { session_id: 'session', session_version: '1' } : { claim_id: 'claim', session_id: 'session' } };
    receipts.set(c.request_id, receipt);
    if (f.afterWrite) await f.afterWrite(c);
    return { receipt };
  };
  f.flow = () => createClaimFlow({ query: f.query, command: f.command,
    save: async a => { saves.push(structuredClone(a)); }, uuid: () => 'request-' + (++seq), current: () => f.current });
  return f;
}
test('claim uses fresh prepare after session creation, current session version and no execution dispatch', async () => {
  const f = fixture(), attempt = {};
  const lease = await f.flow().run(scope, attempt);
  assert.equal(lease.lease_live, true);
  assert.deepEqual(f.writes.map(c => c.op), ['session.start', 'claim.acquire']);
  assert.equal(f.writes[1].expected_project_revision, '2');
  assert.equal(f.writes[1].args.expected_session_version, '3');
  assert.equal(f.writes[1].args.ttl_seconds, 3600);
  assert.equal(f.saves[0].sessionCommand.request_id, f.writes[0].request_id);
  await f.flow().run(scope, JSON.parse(JSON.stringify(attempt)));
  assert.equal(f.writes.length, 2, 'reloading inspects the existing lease');
});
for (const op of ['session.start', 'claim.acquire']) {
  test('lost response after ' + op + ' recovers the receipt without duplicating the write', async () => {
    const f = fixture(), attempt = {};
    f.afterWrite = c => { if (c.op === op) throw Object.assign(Error('network'), { code: 'BridgeUnreachable' }); };
    await assert.rejects(f.flow().run(scope, attempt), /network/);
    f.afterWrite = null;
    await f.flow().run(scope, JSON.parse(JSON.stringify(attempt)));
    assert.equal(f.writes.filter(c => c.op === op).length, 1);
  });
}
test('unknown uncommitted outcome retries only the exact persisted envelope', async () => {
  const f = fixture(), attempt = {};
  f.beforeWrite = () => { throw Object.assign(Error('network'), { code: 'BridgeUnreachable' }); };
  await assert.rejects(f.flow().run(scope, attempt));
  const original = structuredClone(attempt.sessionCommand);
  f.beforeWrite = null;
  await f.flow().run(scope, attempt);
  assert.deepEqual(f.writes[0], original);
});
test('a claim conflict preserves the existing session and prepares a new claim only on explicit retry', async () => {
  const f = fixture(), attempt = {};
  f.beforeWrite = c => { if (c.op === 'claim.acquire') throw Object.assign(Error('held'), { code: 'ClaimHeld' }); };
  await assert.rejects(f.flow().run(scope, attempt), /held/);
  assert.ok(attempt.session);
  assert.equal(attempt.claimCommand, undefined);
  f.beforeWrite = null;
  await f.flow().run(scope, attempt);
  assert.equal(f.writes.filter(c => c.op === 'session.start').length, 1);
});
test('expired receipts cannot be presented as a live claim or silently renewed', async () => {
  const f = fixture(), attempt = {};
  await f.flow().run(scope, attempt);
  f.lease.lease_live = false;
  assert.equal((await f.flow().run(scope, attempt)).lease_live, false);
  assert.equal(f.writes.length, 2);
});
test('scope or contract change and storage failure prevent all writes', async () => {
  const f = fixture();
  await assert.rejects(f.flow().run({ ...scope, contract: 'changed' }, {}), { code: 'SourceChanged' });
  f.current = false;
  await assert.rejects(f.flow().run(scope, {}), { code: 'SelectionChanged' });
  f.current = true;
  const flow = createClaimFlow({ query: f.query, command: f.command, uuid: () => 'id', save: () => { throw Error('quota'); } });
  await assert.rejects(flow.run(scope, {}), /quota/);
  assert.equal(f.writes.length, 0);
});
test('foreign claims cannot become an owned handoff', async () => {
  const f = fixture(), attempt = {};
  await f.flow().run(scope, attempt);
  f.lease.owned_by_client = false;
  await assert.rejects(f.flow().inspect(scope, attempt), { code: 'ForeignClaim' });
});
