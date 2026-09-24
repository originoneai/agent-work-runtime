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
