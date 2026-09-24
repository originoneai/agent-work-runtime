'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { install } = require('./fixtures/dom-stub.js');
const i18n = require('../public/i18n.js');

function loadApp() {
  install();
  delete require.cache[require.resolve('../public/app.js')];
  return require('../public/app.js');
}

test('session and event panels render CLI JSON and hide on Unsupported', () => {
  const app = loadApp();
  const $ = (id) => document.getElementById(id);

  app.applyListResponse('sessions', {
    ok: true,
    data: {
      sessions: [{
        id: 11,
        agent_id: 'lin',
        provider: 'cursor',
        model: 'composer',
        status: 'active',
        work_item_id: 1,
        last_checkpoint_id: 7,
        started_at: Date.now() - 60000,
      }],
    },
  }, 'sessions', 'session.list');
  app.applyListResponse('events', {
    ok: true,
    data: {
      events: [{
        id: 21,
        type: 'session_started',
        summary: 'session started',
        importance: 'high',
        session_id: 11,
        work_item_id: 1,
        created_at: Date.now() - 120000,
      }],
    },
  }, 'events', 'event.history');
  app.renderSessions();
  app.renderEvents();

  assert.equal($('sessionPanel').hidden, false);
  assert.ok($('sessionList').textContent.includes('lin'));
  assert.ok($('sessionList').textContent.includes('active'));
  assert.ok($('sessionList').textContent.includes('checkpoint=7'));
  assert.equal($('eventPanel').hidden, false);
  assert.ok($('eventList').textContent.includes('session started'));
  assert.ok($('eventList').textContent.includes('session_started'));

  app.applyListResponse('sessions', {
    ok: false,
    error: { code: 'Unsupported', message: 'operation is not implemented' },
  }, 'sessions', 'session.list');
  app.applyListResponse('events', {
    ok: false,
    error: { code: 'Unsupported', message: 'operation is not implemented' },
  }, 'events', 'event.history');
  app.renderSessions();
  app.renderEvents();
  assert.equal($('sessionPanel').hidden, true);
  assert.equal($('eventPanel').hidden, true);
  assert.ok(!($('sessionList').textContent.includes(i18n.t('ui.no_active_sessions'))));
});

test('empty session and event lists stay visible', () => {
  const app = loadApp();
  const $ = (id) => document.getElementById(id);
  app.applyListResponse('sessions', { ok: true, data: { sessions: [] } }, 'sessions', 'session.list');
  app.applyListResponse('events', { ok: true, data: { events: [] } }, 'events', 'event.history');
  app.renderSessions();
  app.renderEvents();
  assert.equal($('sessionPanel').hidden, false);
  assert.ok($('sessionList').textContent.includes(i18n.t('ui.no_active_sessions')));
  assert.equal($('eventPanel').hidden, false);
  assert.ok($('eventList').textContent.includes(i18n.t('ui.no_recent_events')));
});
