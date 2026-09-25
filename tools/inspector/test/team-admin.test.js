const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const { webcrypto, createHash } = require('node:crypto');
const { install } = require('./fixtures/dom-stub');
const { createTeamAdmin } = require('../public/team-admin');
const { generateCredential } = require('../public/team-onboarding');
const i18n = require('../public/i18n');
let ui, calls, handler, copied;
const node = id => document.getElementById(id);
const find = (id, tag, text) => node(id).find(n => n.tagName === tag && (!text || n.textContent === text));
const click = (id, text) => { const b = find(id, 'BUTTON', text); assert.ok(b, text); return b.click(); };
const context = { project: 'test', session: 'session', mcpUrl: 'https://team.example/v1/projects/test/mcp',
  identity: { actor_id: 'admin', client_id: 'admin-client', can_manage_members: true, can_read_project_audit: true } };
const directory = { ok: true, data: { items: [{ actor_id: 'alex', display_name: 'Alex', kind: 'human', role: 'developer', independent_review: false,
  clients: [{ client_id: 'alex-agent', credentials: [], grants: [{ workstream_id: 'stream', authority_version: '1', read: true, write: true, manage: false, active: true }] }] }],
  workstreams: [{ id: 'stream', authority_version: '1', write: true }], next_cursor: null } };
beforeEach(() => {
  install(); i18n.setLocale('en'); calls = []; copied = '';
  window.crypto = webcrypto; window.navigator = { clipboard: { writeText: async text => { copied = text; } } };
  navigator.clipboard = window.navigator.clipboard;
  handler = async (op, payload) => op === 'inspect' ? directory : op === 'preview'
    ? { ok: true, data: { state_digest: 'state', plan_digest: 'plan', desired: { subject: payload.plan.subject } } }
    : { ok: true, data: { replayed: false } };
  ui = createTeamAdmin({ $: node, i18n, onAuthError: () => ui.reset(), api: async (url, options) => {
    const b = options ? JSON.parse(options.body) : null; calls.push({ url, body: b });
    return handler(b && b.operation, b && b.payload, url);
  } });
  ui.setContext(context);
});
test('credential generation matches the server hash and is fresh each time', async () => {
  const a = await generateCredential(webcrypto), b = await generateCredential(webcrypto);
  assert.match(a.bearer, /^awr1\.member-[0-9a-f]{24}\.[0-9a-f]{64}$/);
  assert.notEqual(a.bearer, b.bearer);
  assert.equal(a.credential.secret_hash, 'sha256:' + createHash('sha256').update('awr-team-credential-v1:' + a.bearer).digest('hex'));
});
test('ordinary members have only project and personal activity tabs', async () => {
  ui.setContext({ ...context, identity: { can_manage_members: false, can_read_project_audit: false } });
  assert.equal(find('teamConsoleTabs', 'BUTTON', 'Members'), null);
  handler = async () => ({ ok: true, data: { scope: 'self', items: [], next_cursor: null } });
  await click('teamConsoleTabs', 'Activity');
  assert.match(node('teamConsolePanel').textContent, /Your activity/);
  assert.equal(node('teamConsolePanel').find(n => n.placeholder === 'Member ID (optional)'), null);
});
test('one-time generic instructions contain only the new personal bearer and never send it to the bridge', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Issue credential');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  await click('teamConsolePanel', 'Confirm changes');
  const area = find('teamConsolePanel', 'TEXTAREA'); assert.ok(area);
  assert.match(area.value, /Authorization: Bearer awr1\.member-/);
  assert.match(area.value, /work.next/); assert.ok(!area.value.includes('Codex'));
  assert.ok(!JSON.stringify(calls).includes('awr1.'));
  await click('teamConsolePanel', 'Copy Agent instructions'); assert.equal(copied, area.value);
  await click('teamConsolePanel', 'Done, clear credential');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.ok(!JSON.stringify(ui).includes('awr1.'));
});
test('unknown issuance checks the original ID before exact retry and keeps one credential', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Issue credential');
  handler = async op => op === 'apply' ? { ok: false, error: { code: 'BridgeUnreachable', message: 'offline' } }
    : { ok: true, data: { outcome: 'unknown' } };
  await click('teamConsolePanel', 'Confirm changes');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Retry exact request'), null);
  await click('teamConsolePanel', 'Check original result');
  handler = async () => ({ ok: true, data: { replayed: true } });
  await click('teamConsolePanel', 'Retry exact request');
  const applies = calls.filter(c => c.body && c.body.operation === 'apply');
  assert.equal(applies.length, 2); assert.deepEqual(applies[0].body, applies[1].body);
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
});
test('logout or project switch clears one-time credentials and ignores late issuance', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Issue credential');
  let resolve; handler = () => new Promise(r => { resolve = r; });
  const applying = click('teamConsolePanel', 'Confirm changes');
  ui.reset(); resolve({ ok: true, data: { replayed: false } }); await applying;
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null); assert.equal(node('teamConsoleTabs').hidden, true);
});
test('untrusted names stay text and locale switch uses translated labels', async () => {
  directory.data.items[0].display_name = '<img src=x onerror=alert(1)>';
  await click('teamConsoleTabs', 'Members');
  assert.ok(find('teamConsolePanel', 'H3', '<img src=x onerror=alert(1)>'));
  i18n.setLocale('zh-CN'); ui.render();
  assert.ok(find('teamConsoleTabs', 'BUTTON', '成员'));
  directory.data.items[0].display_name = 'Alex';
});
