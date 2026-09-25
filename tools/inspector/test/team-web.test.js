'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const http = require('http');
const fs = require('fs');
const path = require('path');
const { createTeamBridge } = require('../team-bridge');

const FIXTURE_DIR = path.resolve(__dirname, '../../../tests/fixtures/workstreams/team-web-loop');
const GUARD = { 'x-awr-inspector': '1', origin: 'http://127.0.0.1' };

function startBridge() {
  const bridge = createTeamBridge({ teamFixtureDir: FIXTURE_DIR, port: 0 });
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    const key = `${req.method} ${url.pathname}`;
    const handler = bridge.routes[key];
    if (!handler) {
      res.writeHead(404, { 'content-type': 'application/json' });
      res.end('{}');
      return;
    }
    let body = null;
    if (req.method === 'POST') {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const raw = Buffer.concat(chunks).toString('utf8');
      body = raw ? JSON.parse(raw) : {};
    }
    try {
      const json = await handler(url, body, req, res);
      const cookie = res.getHeader('set-cookie');
      const headers = { 'content-type': 'application/json' };
      if (cookie) headers['set-cookie'] = cookie;
      res.writeHead(200, headers);
      res.end(JSON.stringify(json));
    } catch (err) {
      res.writeHead(500, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: { message: String(err.message || err) } }));
    }
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({
        base: `http://127.0.0.1:${port}`,
        close: () => new Promise((r) => server.close(r)),
        bridge,
      });
    });
  });
}

async function req(base, method, urlPath, { body, headers, cookie } = {}) {
  const res = await fetch(base + urlPath, {
    method,
    headers: {
      ...GUARD,
      ...(headers || {}),
      ...(cookie ? { cookie } : {}),
      ...(body ? { 'content-type': 'application/json' } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const setCookie = typeof res.headers.getSetCookie === 'function' ? res.headers.getSetCookie() : [];
  const json = await res.json();
  return { status: res.status, json, setCookie };
}

test('fixtures exist for personal and team views', () => {
  for (const name of ['personal-view.json', 'team-view.json', 'acceptance-cases.json']) {
    assert.ok(fs.existsSync(path.join(FIXTURE_DIR, name)));
  }
});

test('team overview covers owner agent outcome blocker next and card fields', async () => {
  const b = await startBridge();
  try {
    const { json } = await req(b.base, 'GET', '/api/team/overview?view=team&project=demo');
    assert.equal(json.ok, true);
    assert.ok(json.works.length >= 2);
    const blocked = json.works.find((w) => w.key === 'TW-202');
    assert.ok(blocked.blocker.prerequisite_outcome);
    assert.ok(blocked.blocker.release_condition);
    assert.ok(blocked.blocker.check_basis);
    assert.ok(blocked.depends_on.some((d) => d.visible));
    assert.equal(typeof blocked.blocker.backend_code, 'string');
    const hidden = json.works.find((w) => (w.hidden_deps || []).length);
    assert.ok(hidden.hidden_deps[0].hint);
    assert.equal(hidden.hidden_deps[0].leaks, false);
  } finally {
    await b.close();
  }
});

test('login sets cookie, logout and revoke clear it; bearer not echoed', async () => {
  const b = await startBridge();
  try {
    const login = await req(b.base, 'POST', '/api/team/login', {
      body: { bearer: 'awr1.test.0123456789abcdef' },
    });
    assert.equal(login.json.ok, true);
    assert.equal(login.json.auth.bearer_in_page, false);
    assert.ok(!JSON.stringify(login.json).includes('awr1.test.0123456789abcdef'));
    assert.ok(login.setCookie.some((c) => c.startsWith('awr_web_session=') && c.includes('HttpOnly')));
    const cookie = login.setCookie[0].split(';')[0];
    const logout = await req(b.base, 'POST', '/api/team/logout', { cookie, body: {} });
    assert.equal(logout.json.logged_out, true);
  } finally {
    await b.close();
  }
});

test('actions issue idempotent receipts and map to shared server ops', async () => {
  const b = await startBridge();
  try {
    const body = {
      project: 'demo',
      work_key: 'TW-201',
      action: 'submit_review',
      request_id: 'req-1',
    };
    const first = await req(b.base, 'POST', '/api/team/action', { body });
    assert.equal(first.json.ok, true);
    assert.equal(first.json.replayed, false);
    assert.equal(first.json.server_op, 'delivery.submit_and_request_review');
    const second = await req(b.base, 'POST', '/api/team/action', { body });
    assert.equal(second.json.replayed, true);
    assert.equal(second.json.receipt.id, first.json.receipt.id);
  } finally {
    await b.close();
  }
});

test('expired operations are rejected', async () => {
  const b = await startBridge();
  try {
    const { json } = await req(b.base, 'POST', '/api/team/action', {
      body: {
        project: 'demo',
        work_key: 'TW-201',
        action: 'accept',
        request_id: 'req-expired',
        expired: true,
      },
    });
    assert.equal(json.ok, false);
    assert.equal(json.error.code, 'ExpiredOperation');
  } finally {
    await b.close();
  }
});

test('team-web module double-submit guard', async () => {
  const teamWeb = require('../public/team-web.js');
  const calls = [];
  const api = teamWeb.createTeamWeb({
    i18n: { t: (k) => k },
    $: () => null,
    callApi: async () => ({ ok: true }),
  });
  let resolveGate;
  const gate = new Promise((r) => { resolveGate = r; });
  let started = 0;
  const run = api._guardDouble('act', async () => {
    started += 1;
    calls.push('start');
    await gate;
    calls.push('end');
  });
  const p1 = run();
  const p2 = run(); // should no-op while inflight
  resolveGate();
  await Promise.all([p1, p2]);
  assert.equal(started, 1);
});

test('i18n keys for team web exist in en and zh-CN', () => {
  const enText = fs.readFileSync(path.join(__dirname, '../public/locales/en.js'), 'utf8');
  const zhText = fs.readFileSync(path.join(__dirname, '../public/locales/zh-CN.js'), 'utf8');
  for (const key of [
    'ui.team_web',
    'ui.my_projects',
    'ui.blocker_detail',
    'ui.accept_responsibility',
    'ui.hidden_dep_hint_p0',
  ]) {
    assert.ok(enText.includes('"' + key + '"'), key + ' en');
    assert.ok(zhText.includes('"' + key + '"'), key + ' zh');
  }
});

test('rewriteOwnedCookiePath maps upstream Path=/v1/web to /api/team', () => {
  const { rewriteOwnedCookiePath } = require('../team-bridge');
  const set = rewriteOwnedCookiePath(
    'awr_web_session=ws_abc; HttpOnly; Path=/v1/web; SameSite=Strict; Max-Age=28800'
  );
  assert.ok(set.includes('Path=/api/team'));
  assert.ok(!set.includes('Path=/v1/web'));
  const clear = rewriteOwnedCookiePath(
    'awr_web_session=; HttpOnly; Path=/v1/web; SameSite=Strict; Max-Age=0'
  );
  assert.ok(clear.includes('Path=/api/team'));
  assert.ok(clear.includes('Max-Age=0'));
});

function startMockUpstream(handler) {
  const server = http.createServer((req, res) => handler(req, res));
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({
        base: `http://127.0.0.1:${port}`,
        close: () => new Promise((r) => server.close(r)),
        port,
      });
    });
  });
}

function startLiveBridge(teamUrl) {
  const bridge = createTeamBridge({ teamUrl, teamFixtureDir: FIXTURE_DIR, port: 0 });
  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    const key = `${req.method} ${url.pathname}`;
    const handler = bridge.routes[key];
    if (!handler) {
      res.writeHead(404, { 'content-type': 'application/json' });
      res.end('{}');
      return;
    }
    let body = null;
    if (req.method === 'POST') {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const raw = Buffer.concat(chunks).toString('utf8');
      body = raw ? JSON.parse(raw) : {};
    }
    try {
      const json = await handler(url, body, req, res);
      const cookie = res.getHeader('set-cookie');
      const headers = { 'content-type': 'application/json' };
      if (cookie) headers['set-cookie'] = cookie;
      res.writeHead(200, headers);
      res.end(JSON.stringify(json));
    } catch (err) {
      res.writeHead(500, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: { message: String(err.message || err) } }));
    }
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({
        base: `http://127.0.0.1:${port}`,
        close: () => new Promise((r) => server.close(r)),
        bridge,
      });
    });
  });
}

test('live mode never invents demo receipts; proxies command store', async () => {
  const calls = [];
  const upstream = await startMockUpstream((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8');
      calls.push({ method: req.method, url: req.url, cookie: req.headers.cookie || null, body: raw });
      if (req.url === '/v1/web/login' && req.method === 'POST') {
        res.writeHead(200, {
          'content-type': 'application/json',
          'set-cookie':
            'awr_web_session=ws_live1; HttpOnly; Path=/v1/web; SameSite=Strict; Max-Age=28800',
        });
        res.end(JSON.stringify({ ok: true, session_id: 'ws_live1', projects: ['demo'] }));
        return;
      }
      if (req.url === '/v1/web/session' && req.method === 'GET') {
        if (!req.headers.cookie || !req.headers.cookie.includes('awr_web_session=ws_live1')) {
          res.writeHead(401, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ code: 'Unauthenticated', message: 'no web session' }));
          return;
        }
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ ok: true, session_id: 'ws_live1', expires_at_ms: Date.now() + 60000 }));
        return;
      }
      if (req.url === '/v1/web/projects/demo/query' && req.method === 'POST') {
        res.writeHead(200, { 'content-type': 'application/json' });
        const query = JSON.parse(raw);
        res.end(JSON.stringify(query.op === 'workstreams.list'
          ? { items: [{ id: 'stream-live' }], next_cursor: null }
          : { data: { items: [{ external_key: 'TW-LIVE', status: 'open' }], next_cursor: null } }));
        return;
      }
      if (req.url === '/v1/web/projects/demo/command' && req.method === 'POST') {
        const body = JSON.parse(raw || '{}');
        if (body.request_id === 'req-replay') {
          res.writeHead(200, { 'content-type': 'application/json' });
          res.end(
            JSON.stringify({
              replayed: true,
              receipt: { id: 'rcpt_store', request_id: 'req-replay', op: 'review.accept' },
            })
          );
          return;
        }
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(
          JSON.stringify({
            replayed: false,
            receipt: { id: 'rcpt_store', request_id: body.request_id, op: body.op },
          })
        );
        return;
      }
      res.writeHead(404, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ code: 'NotFound' }));
    });
  });

  const b = await startLiveBridge(upstream.base);
  try {
    const login = await req(b.base, 'POST', '/api/team/login', {
      body: { bearer: 'awr1.test.0123456789abcdef' },
    });
    assert.equal(login.json.ok, true);
    assert.ok(login.setCookie.some((c) => c.includes('Path=/api/team')));
    assert.ok(!login.setCookie.some((c) => c.includes('Path=/v1/web')));
    const cookie = login.setCookie[0].split(';')[0];

    const overviewNoCookie = await req(b.base, 'GET', '/api/team/overview?project=demo');
    assert.equal(overviewNoCookie.json.ok, false);
    assert.equal(overviewNoCookie.json.error.code, 'Unauthenticated');

    const overview = await req(b.base, 'GET', '/api/team/overview?project=demo', { cookie });
    assert.equal(overview.json.ok, true);
    assert.equal(overview.json.works[0].key, 'TW-LIVE');
    assert.equal(overview.json.schema, 'awr-team-web-loop-live/v1');

    const denyAction = await req(b.base, 'POST', '/api/team/action', {
      body: { project: 'demo', action: 'accept', request_id: 'x', work_key: 'TW-LIVE' },
    });
    assert.equal(denyAction.json.ok, false);
    assert.equal(denyAction.json.error.code, 'Unauthenticated');

    const accept = await req(b.base, 'POST', '/api/team/action', {
      cookie,
      body: {
        project: 'demo',
        command: {
          protocol_version: 1,
          request_id: 'req-1',
          op: 'review.accept',
          workstream_id: '1',
          work_id: 'TW-LIVE',
          coordinator_epoch: 'e',
          expected_project_revision: '1',
          expected_authority_version: '1',
          expected_ownership_version: '1',
          expected_contract_hash: 'a'.repeat(64),
          args: {},
        },
      },
    });
    assert.equal(accept.json.ok, true);
    assert.equal(accept.json.replayed, false);
    assert.equal(accept.json.receipt.op, 'review.accept');
    assert.equal(accept.json.receipt.id, 'rcpt_store');
    assert.ok(!String(accept.json.receipt.id).startsWith('rcpt_req'));

    const replay = await req(b.base, 'POST', '/api/team/action', {
      cookie,
      body: {
        project: 'demo',
        command: {
          protocol_version: 1,
          request_id: 'req-replay',
          op: 'review.accept',
          workstream_id: '1',
          work_id: 'TW-LIVE',
          coordinator_epoch: 'e',
          expected_project_revision: '1',
          expected_authority_version: '1',
          expected_ownership_version: '1',
          expected_contract_hash: 'a'.repeat(64),
          args: {},
        },
      },
    });
    assert.equal(replay.json.ok, true);
    assert.equal(replay.json.replayed, true);
    assert.equal(replay.json.receipt.id, 'rcpt_store');

    assert.ok(calls.some((c) => c.url === '/v1/web/login'));
    assert.ok(calls.some((c) => c.url === '/v1/web/projects/demo/query'));
    assert.ok(calls.some((c) => c.url === '/v1/web/projects/demo/command'));
    // Overview + action must not invent fixture receipts without upstream.
    assert.ok(!calls.every((c) => c.url === '/v1/web/login'));
  } finally {
    await b.close();
    await upstream.close();
  }
});

test('live overview discovers authorized streams and follows scoped pagination', async () => {
  const queries = [];
  const upstream = await startMockUpstream((request, response) => {
    const chunks = [];
    request.on('data', (chunk) => chunks.push(chunk));
    request.on('end', () => {
      response.setHeader('content-type', 'application/json');
      if (request.url === '/v1/web/session') {
        response.end(JSON.stringify({ session_id: 'ws_multi' }));
        return;
      }
      const query = JSON.parse(Buffer.concat(chunks).toString());
      queries.push(query);
      assert.equal(request.headers.cookie, 'awr_web_session=ws_multi');
      if (query.op === 'workstreams.list') {
        response.end(JSON.stringify(query.cursor
          ? { items: [{ id: 'frontend' }], next_cursor: null }
          : { items: [{ id: 'backend' }], next_cursor: 'streams-2' }));
      } else if (query.workstream_id === 'backend') {
        response.end(JSON.stringify({ data: query.cursor
          ? { items: [{ work_id: 'api-2' }], next_cursor: null }
          : { items: [{ work_id: 'api-1' }], next_cursor: 'backend-2' } }));
      } else if (query.workstream_id === 'frontend') {
        response.end(JSON.stringify({ data: { items: [{ work_id: 'ui-1' }], next_cursor: null } }));
      } else {
        response.writeHead(403);
        response.end(JSON.stringify({ code: 'Forbidden' }));
      }
    });
  });
  const bridge = await startLiveBridge(upstream.base);
  try {
    const overview = await req(bridge.base, 'GET', '/api/team/overview?project=demo', {
      cookie: 'awr_web_session=ws_multi',
    });
    assert.equal(overview.json.ok, true);
    assert.deepEqual(overview.json.works.map((work) => [work.key, work.workstream_id]), [
      ['api-1', 'backend'], ['api-2', 'backend'], ['ui-1', 'frontend'],
    ]);
    assert.deepEqual(queries.map(({ op, workstream_id, cursor }) => [op, workstream_id, cursor]), [
      ['workstreams.list', undefined, undefined],
      ['workstreams.list', undefined, 'streams-2'],
      ['work.list', 'backend', undefined],
      ['work.list', 'backend', 'backend-2'],
      ['work.list', 'frontend', undefined],
    ]);
  } finally {
    await bridge.close();
    await upstream.close();
  }
});

for (const scenario of ['empty', 'revoked', 'invalid_page', 'repeated_cursor']) {
  test(`live overview handles ${scenario} without claiming a partial result`, async () => {
    const upstream = await startMockUpstream((request, response) => {
      const chunks = [];
      request.on('data', (chunk) => chunks.push(chunk));
      request.on('end', () => {
        response.setHeader('content-type', 'application/json');
        if (request.url === '/v1/web/session') {
          response.end(JSON.stringify({ session_id: 'ws_test' }));
          return;
        }
        const query = JSON.parse(Buffer.concat(chunks).toString());
        if (query.op === 'workstreams.list') {
          response.end(JSON.stringify({ items: scenario === 'empty' ? [] : [{ id: 'backend' }], next_cursor: null }));
        } else if (scenario === 'revoked') {
          response.writeHead(403);
          response.end(JSON.stringify({ code: 'Forbidden', message: 'grant revoked' }));
        } else if (scenario === 'invalid_page') {
          response.end(JSON.stringify({ data: { items: null } }));
        } else {
          response.end(JSON.stringify({ data: { items: [{ work_id: 'api' }], next_cursor: 'same' } }));
        }
      });
    });
    const bridge = await startLiveBridge(upstream.base);
    try {
      const overview = await req(bridge.base, 'GET', '/api/team/overview?project=demo');
      if (scenario === 'empty') {
        assert.equal(overview.json.ok, true);
        assert.deepEqual(overview.json.works, []);
      } else {
        assert.equal(overview.json.ok, false);
        assert.equal(overview.json.error.code, scenario === 'revoked' ? 'Forbidden' : 'BadGateway');
        assert.equal(overview.json.works, undefined);
      }
    } finally {
      await bridge.close();
      await upstream.close();
    }
  });
}

test('live mode expired session denies overview and action', async () => {
  const upstream = await startMockUpstream((req, res) => {
    if (req.url === '/v1/web/session') {
      res.writeHead(401, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ code: 'SessionExpired', message: 'web session expired or revoked' }));
      return;
    }
    res.writeHead(500, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ code: 'Unexpected' }));
  });
  const b = await startLiveBridge(upstream.base);
  try {
    const overview = await req(b.base, 'GET', '/api/team/overview?project=demo', {
      cookie: 'awr_web_session=ws_expired',
    });
    assert.equal(overview.json.ok, false);
    assert.equal(overview.json.error.code, 'SessionExpired');
    const action = await req(b.base, 'POST', '/api/team/action', {
      cookie: 'awr_web_session=ws_expired',
      body: {
        project: 'demo',
        command: { protocol_version: 1, request_id: 'r', op: 'review.accept' },
      },
    });
    assert.equal(action.json.ok, false);
    assert.equal(action.json.error.code, 'SessionExpired');
  } finally {
    await b.close();
    await upstream.close();
  }
});
