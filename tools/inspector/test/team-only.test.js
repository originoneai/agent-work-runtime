'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { install, StubElement } = require('./fixtures/dom-stub');
install();
const app = require('../public/app');

test('Team-only bootstrap never requests local project data and hides unavailable navigation', async () => {
  const links = ['overview', 'work', 'context', 'mainline', 'sources', 'team'].map((view) => {
    const link = new StubElement('a'); link.dataset.view = view; return link;
  });
  document.querySelectorAll = (selector) => selector === '.rail a' ? links : [];
  const calls = [];
  global.fetch = async (url) => {
    calls.push(url);
    assert.equal(url, '/api/health');
    return { ok: true, json: async () => ({ ok: true, data: { teamOnly: true, mode: 'team', project: null } }) };
  };
  await app.loadAll();
  assert.deepEqual(calls, ['/api/health']);
  assert.equal(app.state.view, 'team');
  assert.deepEqual(links.map((link) => link.hidden), [true, true, true, true, true, false]);
  assert.equal(document.getElementById('projectPicker').hidden, true);
  assert.equal(document.getElementById('btnGuide').hidden, true);
  assert.equal(document.getElementById('view-overview').hidden, true);
  assert.equal(document.getElementById('view-team').hidden, false);
});
