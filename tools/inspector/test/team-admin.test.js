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
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Generate connection instructions');
  const plan = calls.find(c => c.body && c.body.operation === 'preview').body.payload.plan;
  assert.ok(plan.grants.every(g => g.attest_execution === false && g.reconcile_execution === false), 'explicit server grant fields must be present without special executor authority');
  assert.equal(calls.filter(c => c.body && c.body.operation === 'apply').length, 1);
  const area = find('teamConsolePanel', 'TEXTAREA'); assert.ok(area);
  assert.match(area.value, /Authorization: Bearer awr1\.member-/);
  assert.match(area.value, /work.next/); assert.ok(!area.value.includes('Codex'));
  assert.ok(!JSON.stringify(calls).includes('awr1.'));
  await click('teamConsolePanel', 'Copy instructions for this member'); assert.equal(copied, area.value);
  await click('teamConsolePanel', 'Done');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.ok(!JSON.stringify(ui).includes('awr1.'));
});
test('unknown issuance checks the original ID before exact retry and keeps one credential', async () => {
  await click('teamConsoleTabs', 'Members');
  const previous = handler;
  handler = async (op, payload) => op === 'apply' ? { ok: false, error: { code: 'BridgeUnreachable' } }
    : op === 'outcome' ? { ok: true, data: { outcome: 'unknown' } } : previous(op, payload);
  await click('teamConsolePanel', 'Generate connection instructions');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Try saving again'), null);
  await click('teamConsolePanel', 'Check whether it was saved');
  handler = async () => ({ ok: true, data: { replayed: true } });
  await click('teamConsolePanel', 'Try saving again');
  const applies = calls.filter(c => c.body && c.body.operation === 'apply');
  assert.equal(applies.length, 2); assert.deepEqual(applies[0].body, applies[1].body);
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
});
test('logout or project switch clears one-time credentials and ignores late issuance', async () => {
  await click('teamConsoleTabs', 'Members');
  const previous = handler;
  let resolve, signal;
  const arrived = new Promise(r => { signal = r; });
  handler = (op, payload) => op === 'apply' ? new Promise(r => { resolve = r; signal(); }) : previous(op, payload);
  const applying = click('teamConsolePanel', 'Generate connection instructions');
  await arrived;
  ui.reset(); resolve({ ok: true, data: { replayed: false } }); await applying;
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null); assert.equal(node('teamConsoleTabs').hidden, true);
});
test('committed access removal refreshes the directory without stale member controls', async () => {
  await click('teamConsoleTabs', 'Members');
  await click('teamConsolePanel', 'Remove member');
  handler = async op => op === 'inspect'
    ? { ok: true, data: { ...directory.data, items: [] } }
    : { ok: true, data: { replayed: false } };
  await click('teamConsolePanel', 'Remove this member');
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Remove member'), null);
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Add member').disabled, false);
  assert.equal(calls.at(-1).body.operation, 'inspect');
});
test('untrusted names stay text and locale switch uses translated labels', async () => {
  directory.data.items[0].display_name = '<img src=x onerror=alert(1)>';
  await click('teamConsoleTabs', 'Members');
  assert.ok(find('teamConsolePanel', 'H3', '<img src=x onerror=alert(1)>'));
  i18n.setLocale('zh-CN'); ui.render();
  assert.ok(find('teamConsoleTabs', 'BUTTON', '成员'));
  directory.data.items[0].display_name = 'Alex';
});

function inputName(text) {
  const label = node('teamConsolePanel').find(n => n.tagName === 'LABEL' && n.children[0]?.textContent === 'Name');
  const input = label.find(n => n.tagName === 'INPUT');
  input.value = text;
  for (const fn of input.listeners.input || []) fn({ target: input });
  return input;
}

test('a name is sufficient to create a member and personal handoff in one explicit action', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Add member');
  assert.equal(node('teamConsolePanel').find(n => n.tagName === 'LABEL' && n.textContent.includes('Member ID')), null);
  inputName('New colleague');
  const submit = find('teamConsolePanel', 'BUTTON', 'Add member and generate connection instructions');
  assert.match(submit.className, /primary/);
  await Promise.all([submit.click(), submit.click()]);
  const previews = calls.filter(c => c.body?.operation === 'preview');
  const applies = calls.filter(c => c.body?.operation === 'apply');
  assert.equal(previews.length, 1); assert.equal(applies.length, 1);
  const plan = applies[0].body.payload.plan;
  assert.match(plan.subject.id, /^member-[0-9a-f-]{36}$/);
  assert.equal(plan.subject.display_name, 'New colleague');
  assert.equal(plan.subject_client_id, plan.subject.id + '-agent');
  assert.equal(plan.credential_project_scoped, true);
  assert.equal(plan.role, 'developer');
  assert.deepEqual(plan.grants, [{ workstream_id: 'stream', authority_version: '1', read: true, write: true, manage: false, attest_execution: false, reconcile_execution: false }]);
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
  assert.match(node('teamConsolePanel').textContent, /New colleague has been added/);
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Confirm changes'), null);
  assert.ok(!JSON.stringify(calls).includes('awr1.'));
});

test('blank names and empty work areas show actionable errors without writes', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Add member');
  inputName('   '); await click('teamConsolePanel', 'Add member and generate connection instructions');
  assert.match(node('teamConsolePanel').textContent, /Enter a member name/);
  inputName('Alex');
  const scopes = find('teamConsolePanel', 'FIELDSET');
  const checkbox = scopes.find(n => n.tagName === 'INPUT'); checkbox.checked = false;
  for (const fn of checkbox.listeners.change) fn({ target: checkbox });
  await click('teamConsolePanel', 'Add member and generate connection instructions');
  assert.match(node('teamConsolePanel').textContent, /Select at least one work area/);
  assert.equal(calls.filter(c => c.body?.operation !== 'inspect').length, 0);
});

test('submitting the form with Enter follows the same creation path', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Add member');
  inputName('Keyboard user');
  const form = find('teamConsolePanel', 'FORM');
  for (const fn of form.listeners.submit) await fn({ preventDefault() {} });
  assert.equal(calls.filter(c => c.body?.operation === 'apply').length, 1);
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
});

test('network exceptions during saving retain one recoverable request and never expose an uncommitted secret', async () => {
  await click('teamConsoleTabs', 'Members');
  const previous = handler;
  handler = (op, payload) => { if (op === 'apply') throw Error('lost connection'); return previous(op, payload); };
  await click('teamConsolePanel', 'Generate connection instructions');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.ok(find('teamConsolePanel', 'BUTTON', 'Check whether it was saved'));
  handler = async () => ({ ok: true, data: { outcome: 'committed' } });
  await click('teamConsolePanel', 'Check whether it was saved');
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
  assert.equal(calls.filter(c => c.body?.operation === 'apply').length, 1);
});

test('a rejected save preserves the member draft for correction', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Add member'); inputName('Keep this name');
  const previous = handler;
  handler = (op, payload) => op === 'apply' ? { ok: false, error: { code: 'PreconditionsChanged' } } : previous(op, payload);
  await click('teamConsolePanel', 'Add member and generate connection instructions');
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  await click('teamConsolePanel', 'Back to edit');
  assert.equal(find('teamConsolePanel', 'INPUT').value, 'Keep this name');
});

test('revoking administrator access clears a prepared personal credential', async () => {
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Generate connection instructions');
  assert.ok(find('teamConsolePanel', 'TEXTAREA'));
  ui.setContext({ ...context, identity: { can_manage_members: false, can_read_project_audit: false } });
  assert.equal(find('teamConsolePanel', 'TEXTAREA'), null);
  assert.equal(find('teamConsoleTabs', 'BUTTON', 'Members'), null);
});

test('replacing a credential describes the impact and waits for explicit confirmation', async () => {
  const data = structuredClone(directory);
  data.data.items[0].clients[0].credentials = [{ id: 'existing', project_scoped: true }];
  const previous = handler;
  handler = (op, payload) => op === 'inspect' ? data : previous(op, payload);
  await click('teamConsoleTabs', 'Members'); await click('teamConsolePanel', 'Replace credential');
  assert.equal(calls.filter(c => c.body?.operation === 'apply').length, 0);
  assert.match(node('teamConsolePanel').textContent, /old credential will stop working/);
  assert.equal(find('teamConsolePanel', 'PRE'), null);
  await click('teamConsolePanel', 'Replace this credential');
  const plan = calls.find(c => c.body?.operation === 'apply').body.payload.plan;
  assert.deepEqual(plan.revoke_project_credentials, ['existing']);
  assert.ok(plan.credential && find('teamConsolePanel', 'TEXTAREA'));
});

test('members without work areas are directed to permissions before credential generation', async () => {
  const data = structuredClone(directory); data.data.items[0].clients[0].grants = [];
  handler = async () => data;
  await click('teamConsoleTabs', 'Members');
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Generate connection instructions').disabled, true);
  assert.match(node('teamConsolePanel').textContent, /Choose work areas/);
  assert.equal(find('teamConsolePanel', 'BUTTON', 'Edit access').disabled, false);
});

test('audit filters display names but send the authoritative member identity', async () => {
  const previous = handler;
  handler = (op, payload) => op === null ? { ok: true, data: { items: [{ actor_id: 'alex', action: 'claim.acquire', result: 'succeeded' }], next_cursor: null } } : previous(op, payload);
  await click('teamConsoleTabs', 'Activity');
  const picker = node('teamConsolePanel').find(n => n.tagName === 'SELECT' && n.getAttribute('aria-label') === 'Member');
  assert.equal(picker.find(n => n.value === 'alex').textContent, 'Alex');
  picker.value = 'alex'; for (const fn of picker.listeners.change) fn();
  await click('teamConsolePanel', 'Apply filters');
  assert.match(calls.at(-1).url, /member_actor_id=alex/);
  assert.match(node('teamConsolePanel').textContent, /Claim a task/);
  assert.match(node('teamConsolePanel').textContent, /Succeeded/);
});
