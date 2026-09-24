'use strict';

const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const { install } = require('./fixtures/dom-stub');
const { createTeamWeb } = require('../public/team-web');
const i18n = require('../public/i18n');
let ui;
const response = (body) => ({ ok: true, json: async () => body });
const project = { key: 'example', title: 'Example project' };
const work = { key: 'WORK-1', title: 'Example work', status: 'planned' };
const projects = { ok: true, projects: [project], session: { session_id: 'test-session' } };
const overview = { ok: true, works: [work], members: [] };
const node = (id) => document.getElementById(id);
const button = (id, label) => node(id).find((el) => el.tagName === 'BUTTON' && el.textContent === label);

function mock(handler) {
  const calls = [];
  global.fetch = async (url, options) => {
    calls.push({ url, options });
    return response(await handler(url, options));
  };
  return calls;
}

beforeEach(() => {
  install();
  i18n.setLocale('en');
  ui = createTeamWeb({ i18n, $: node });
});

test('successful login loads projects and work immediately and clears the credential field', async () => {
  const calls = mock((url) => url.endsWith('/login') ? { ok: true, session_id: 'test-session' }
    : url.includes('/projects?') ? projects : overview);
  ui.render();
  const input = node('teamAuth').find((el) => el.tagName === 'INPUT');
  input.value = 'synthetic-login-value';
  await button('teamAuth', 'Sign in').click();
  assert.equal(input.value, '');
  assert.deepEqual(ui.state.works, [work]);
  assert.match(node('teamProjects').textContent, /Example project/);
  assert.equal(calls.length, 3);
  assert.ok(!JSON.stringify(ui.state).includes('synthetic-login-value'));
});

test('failed login displays an error instead of silently returning to the form', async () => {
  mock(() => ({ ok: false, error: { code: 'Forbidden', message: 'access denied' } }));
  ui.render();
  await button('teamAuth', 'Sign in').click();
  assert.match(node('teamAuth').textContent, /Forbidden.*access denied/);
  assert.equal(ui.state.session, null);
});

test('an anonymous refresh preserves a credential draft until the user submits it', async () => {
  let release;
  const pending = new Promise((resolve) => { release = resolve; });
  mock((url) => url.includes('/projects?') ? pending : { ok: false });
  const refreshing = ui.refresh();
  const input = node('teamAuth').find((el) => el.tagName === 'INPUT');
  input.value = 'synthetic-login-draft';
  let focusRestored = 0;
  document.activeElement = input;
  input.focus = () => { focusRestored++; };
  release({ ok: false, error: { code: 'Unauthenticated', message: 'cookie required' } });
  await refreshing;
  assert.equal(node('teamAuth').find((el) => el.tagName === 'INPUT'), input);
  assert.equal(input.value, 'synthetic-login-draft');
  assert.equal(focusRestored, 1);
  const calls = mock((url) => url.endsWith('/login') ? { ok: true, session_id: 'test-session' }
    : url.includes('/projects?') ? projects : overview);
  await button('teamAuth', 'Sign in').click();
  assert.equal(JSON.parse(calls[0].options.body).bearer, 'synthetic-login-draft');
  assert.equal(input.value, '');
  assert.ok(!JSON.stringify(ui.state).includes('synthetic-login-draft'));
  assert.deepEqual(ui.state.works, [work]);
});

for (const label of ['Log out', 'Revoke sessions']) {
  test(`${label} removes project data, selected detail and receipts`, async () => {
    mock((url) => url.includes('/projects?') ? projects : url.includes('/overview?') ? overview : { ok: true });
    await ui.refresh();
    ui.state.selected = work.key;
    ui.state.lastReceipts.saved = 'receipt';
    ui.render();
    await button('teamAuth', label).click();
    assert.equal(ui.state.session, null);
    assert.equal(ui.state.projectKey, null);
    assert.deepEqual(ui.state.projects, []);
    assert.deepEqual(ui.state.works, []);
    assert.equal(ui.state.selected, null);
    assert.equal(ui.state.raw, null);
    assert.equal(node('rawTeamBody').textContent, '');
    assert.equal(Object.keys(ui.state.lastReceipts).length, 0);
    assert.ok(!node('teamDetail').textContent.includes('Example work'));
  });
}

test('a denied refresh clears stale data and renders the error', async () => {
  mock((url) => url.includes('/projects?') ? projects : overview);
  await ui.refresh();
  mock(() => ({ ok: false, error: { code: 'SessionExpired', message: 'expired' } }));
  await ui.refresh();
  assert.equal(ui.state.session, null);
  assert.deepEqual(ui.state.works, []);
  assert.deepEqual(ui.state.projects, []);
  assert.match(node('teamAuth').textContent, /SessionExpired/);
});

test('the first anonymous visit is a sign-in state and raw JSON omits cookie identifiers', async () => {
  mock(() => ({ ok: false, error: { code: 'Unauthenticated', message: 'cookie required' } }));
  await ui.refresh();
  assert.equal(ui.state.error, null);
  mock((url) => url.includes('/projects?') ? projects
    : { ...overview, session: { session_id: 'secret-cookie-identifier' } });
  await ui.refresh();
  assert.match(node('rawTeamBody').textContent, /Example work/);
  assert.ok(!node('rawTeamBody').textContent.includes('secret-cookie-identifier'));
});

test('an overview failure is visible and never leaves old work on screen', async () => {
  mock((url) => url.includes('/projects?') ? projects : overview);
  await ui.refresh();
  mock((url) => url.includes('/projects?') ? projects : { ok: false, error: { code: 'Forbidden', message: 'grant revoked' } });
  await ui.refresh();
  assert.deepEqual(ui.state.works, []);
  assert.match(node('teamAuth').textContent, /grant revoked/);
});

test('a late refresh cannot restore data after logout', async () => {
  mock((url) => url.includes('/projects?') ? projects : overview);
  await ui.refresh();
  let resolve;
  const gate = new Promise((r) => { resolve = r; });
  mock((url) => url.includes('/projects?') ? gate : { ok: true });
  const pending = ui.refresh();
  await button('teamAuth', 'Log out').click();
  resolve(projects);
  await pending;
  assert.equal(ui.state.session, null);
  assert.deepEqual(ui.state.projects, []);
  assert.deepEqual(ui.state.works, []);
});

test('a late project response cannot replace the latest selection', async () => {
  let resolve;
  const gate = new Promise((r) => { resolve = r; });
  mock(() => gate);
  const old = ui.refresh();
  mock((url) => url.includes('/projects?') ? { ...projects, projects: [{ key: 'new' }] }
    : { ok: true, works: [{ key: 'NEW' }] });
  await ui.refresh();
  resolve(projects);
  await old;
  assert.equal(ui.state.projectKey, 'new');
  assert.deepEqual(ui.state.works, [{ key: 'NEW' }]);
});

test('failed action shows the server error and creates no success receipt', async () => {
  mock((url) => url.includes('/projects?') ? projects : overview);
  await ui.refresh();
  ui.state.selected = work.key;
  ui.render();
  mock(() => ({ ok: false, error: { code: 'InvalidInput', message: 'command required' } }));
  await button('teamDetail', 'Accept responsibility').click();
  assert.match(node('teamAuth').textContent, /InvalidInput.*command required/);
  assert.equal(Object.keys(ui.state.lastReceipts).length, 0);
});

test('transport failure explains unknown outcome without replaying the operation', async () => {
  let calls = 0;
  global.fetch = async () => { calls++; throw new Error('offline'); };
  ui.state.works = [work];
  ui.state.selected = work.key;
  ui.render();
  await button('teamDetail', 'Accept responsibility').click();
  assert.equal(calls, 1);
  assert.match(node('teamAuth').textContent, /BridgeUnreachable/);
  assert.match(node('teamAuth').textContent, /Inspect the original operation/);
});

test('live work uses authorized details and directs unsupported writes to MCP', async () => {
  const liveWork = { ...work, workstream_id: 'stream', detail_loaded: false };
  const detail = { ...liveWork, detail_loaded: true, runtime_available: false,
    status: null, acceptance: ['A real contract criterion'], depends_on: [],
    dependency_export_unavailable: true, context_complete: false,
    completeness_reasons: ['dependency_export_unavailable'] };
  const calls = mock((url) => url.includes('/projects?') ? projects
    : url.includes('/overview?') ? { ...overview, works: [liveWork], interaction_mode: 'mcp' }
    : { ok: true, work: detail });
  await ui.refresh();
  const card = node('teamOverview').find((el) => el.tagName === 'ARTICLE');
  assert.equal(card.getAttribute('tabindex'), '0');
  await card.click();
  assert.match(node('teamDetail').textContent, /A real contract criterion/);
  assert.match(node('teamDetail').textContent, /No execution recorded/);
  assert.match(node('teamDetail').textContent, /Readiness cannot be confirmed/);
  assert.match(node('teamDetail').textContent, /MCP/);
  assert.ok(!button('teamDetail', 'Accept responsibility'));
  assert.equal(button('teamOverview', 'Personal').disabled, true);
  assert.match(calls[2].url, /workstream=stream/);
});

test('late details for a previously selected work never replace the latest selection', async () => {
  const a = { ...work, workstream_id: 'stream' };
  const b = { key: 'WORK-2', title: 'Second work', workstream_id: 'stream' };
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const detail = (item) => ({ ok: true, work: { ...item, detail_loaded: true,
    acceptance: [item.title], depends_on: [], context_complete: true } });
  mock((url) => url.includes('/projects?') ? projects
    : url.includes('/overview?') ? { ok: true, works: [a, b], interaction_mode: 'mcp' }
    : url.includes('work=WORK-1') ? gate : detail(b));
  await ui.refresh();
  const pending = node('teamOverview').find((el) => el.tagName === 'ARTICLE' && el.dataset.key === 'WORK-1').click();
  await node('teamOverview').find((el) => el.tagName === 'ARTICLE' && el.dataset.key === 'WORK-2').click();
  release(detail(a));
  await pending;
  assert.equal(ui.state.selected, 'WORK-2');
  assert.ok(!node('teamDetail').textContent.includes('Example work'));
});
