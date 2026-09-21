/**
 * 桥接的测试。用 node:test，零依赖。
 *
 * 每个用例起一个真实的 server.js 子进程，PATH 上放一个假 awr，
 * 然后用 fetch 打真实的 HTTP 请求——测的是真实的请求边界，不是内部函数。
 *
 * 跑：node --test test/
 */

'use strict';

const { test, before, after } = require('node:test');
const assert = require('node:assert');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');

const ROOT = path.join(__dirname, '..');
const GUARD = { 'x-awr-inspector': '1' };

/** 造一个 bin 目录，里面的 `awr` 指向 stub。 */
function makeStubBin() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'awr-stub-'));
  const stub = path.join(__dirname, 'fixtures', 'stub-awr.js');
  if (process.platform === 'win32') {
    const bin = path.join(dir, 'awr.cmd');
    fs.writeFileSync(bin, `"${process.execPath}" "${stub}" %*\r\n`);
    return dir;
  }
  const bin = path.join(dir, 'awr');
  fs.writeFileSync(bin, `#!/bin/sh\nexec "${process.execPath}" "${stub}" "$@"\n`);
  fs.chmodSync(bin, 0o755);
  return dir;
}

const STUB_BIN = makeStubBin();
let nextPort = 7500;

/** 起一个桥接进程，等它监听上，返回 { port, stop }。 */
async function startBridge(opts = {}) {
  const port = nextPort++;
  const args = ['server.js', '--no-open', '--port', String(port)];
  const project = opts.project || ROOT;
  args.push('--project', project);
  if (opts.allowReindex) args.push('--allow-reindex');
  if (opts.demo) args.push('--demo');

  const sep = process.platform === 'win32' ? ';' : ':';
  const child = spawn(process.execPath, args, {
    cwd: ROOT,
    env: Object.assign({}, process.env, opts.env, { PATH: `${STUB_BIN}${sep}${process.env.PATH}` }),
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('桥接启动超时')), 15000);
    child.stdout.on('data', (d) => {
      if (String(d).includes('已启动')) {
        clearTimeout(timer);
        resolve();
      }
    });
    child.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`桥接退出了，code=${code}`));
    });
  });

  return {
    port,
    base: `http://127.0.0.1:${port}`,
    stop: () => new Promise((r) => { child.on('exit', r); child.kill('SIGKILL'); }),
  };
}

let bridge;

before(async () => { bridge = await startBridge(); });
after(async () => { if (bridge) await bridge.stop(); fs.rmSync(STUB_BIN, { recursive: true, force: true }); });

// ───────────── 1. 请求来源边界 ─────────────

/** fetch 不允许覆盖 Host 头，所以敌对 Host 只能用原始请求构造。 */
function rawRequest(port, options) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      Object.assign({ host: '127.0.0.1', port, method: 'GET' }, options),
      (res) => {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () =>
          resolve({ status: res.statusCode, text: Buffer.concat(chunks).toString('utf8') })
        );
      }
    );
    req.on('error', reject);
    if (options && options.body) req.write(options.body);
    req.end();
  });
}

test('拒绝敌对 Host（DNS rebinding）', async () => {
  const res = await rawRequest(bridge.port, {
    path: '/api/health',
    headers: { Host: 'attacker.example' },
  });
  assert.equal(res.status, 403);
  const body = JSON.parse(res.text);
  assert.equal(body.error.code, 'ForbiddenHost');
  assert.ok(!res.text.includes('agent-work-runtime'), '不得泄露项目路径');
});

test('接受 localhost 形式的 Host', async () => {
  const res = await rawRequest(bridge.port, {
    path: '/api/health',
    headers: { Host: `localhost:${bridge.port}` },
  });
  assert.equal(res.status, 200);
});

test('拒绝敌对 Origin', async () => {
  const res = await fetch(`${bridge.base}/api/status`, {
    headers: Object.assign({ origin: 'https://example.attacker' }, GUARD),
  });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, 'ForbiddenOrigin');
});

test("拒绝 Origin: null", async () => {
  const res = await fetch(`${bridge.base}/api/status`, { headers: { origin: 'null' } });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, 'ForbiddenOrigin');
});

test('拒绝跨站 Sec-Fetch-Site', async () => {
  const res = await fetch(`${bridge.base}/api/status`, {
    headers: { 'sec-fetch-site': 'cross-site' },
  });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, 'ForbiddenSite');
});

test('接受同源 Sec-Fetch-Site', async () => {
  const res = await fetch(`${bridge.base}/api/status`, {
    headers: { 'sec-fetch-site': 'same-origin' },
  });
  assert.equal(res.status, 200);
  assert.equal((await res.json()).ok, true);
});

test('跨站表单 POST 触发不了 reindex', async () => {
  const res = await fetch(`${bridge.base}/api/source/reindex`, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: 'x=1',
  });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, 'MissingGuardHeader');
});

test('静态资源拒绝目录穿越', async () => {
  const res = await fetch(`${bridge.base}/../server.js`);
  assert.ok(res.status === 403 || res.status === 404, `期望 403/404，实际 ${res.status}`);
});

test('静态响应带 CSP', async () => {
  const res = await fetch(`${bridge.base}/`);
  const csp = res.headers.get('content-security-policy');
  assert.ok(csp && csp.includes("default-src 'self'"), 'CSP 头缺失');
  assert.ok(!csp.includes('unsafe-inline'), 'CSP 不应放行内联');
});

// ───────────── 2. 命令构造 ─────────────

test('search 用位置参数，不用 --text', async () => {
  const argvLog = path.join(os.tmpdir(), `argv-${Date.now()}.log`);
  const b = await startBridge({ env: { STUB_ARGV_OUT: argvLog } });
  try {
    const res = await fetch(`${b.base}/api/search?text=source`, { headers: GUARD });
    const body = await res.json();
    assert.equal(body.ok, true, JSON.stringify(body));
    assert.equal(body.data.query.text, 'source');

    const lines = fs.readFileSync(argvLog, 'utf8').trim().split('\n').map(JSON.parse);
    const call = lines.find((a) => a.includes('search'));
    assert.ok(!call.includes('--text'), '不应再出现 --text');
    assert.ok(call.includes('--'), '应该用 -- 分隔位置参数');
  } finally {
    await b.stop();
    fs.rmSync(argvLog, { force: true });
  }
});

test('search 接受中文', async () => {
  const res = await fetch(`${bridge.base}/api/search?text=${encodeURIComponent('任务')}`, {
    headers: GUARD,
  });
  const body = await res.json();
  assert.equal(body.ok, true, JSON.stringify(body));
  assert.equal(body.data.query.text, '任务');
});

test('search 接受以 - 开头的词', async () => {
  const res = await fetch(`${bridge.base}/api/search?text=${encodeURIComponent('-flag')}`, {
    headers: GUARD,
  });
  const body = await res.json();
  assert.equal(body.ok, true, JSON.stringify(body));
  assert.equal(body.data.query.text, '-flag');
});

test('search 拒绝控制字符', async () => {
  const withNul = 'a' + String.fromCharCode(0) + 'b';
  const res = await fetch(`${bridge.base}/api/search?text=${encodeURIComponent(withNul)}`, {
    headers: GUARD,
  });
  assert.equal((await res.json()).error.code, 'BadRequest');
});

test('一次坏请求不会改变 --json 的位置', async () => {
  const argvLog = path.join(os.tmpdir(), `argv2-${Date.now()}.log`);
  const b = await startBridge({ env: { STUB_ARGV_OUT: argvLog } });
  try {
    // 先打一个会让 stub 报 "unexpected argument" 的请求
    await fetch(`${b.base}/api/work?key=NOPE%3B`, { headers: GUARD });
    // 再打一个正常请求，--json 仍应在全局位置
    await fetch(`${b.base}/api/status`, { headers: GUARD });
    const lines = fs.readFileSync(argvLog, 'utf8').trim().split('\n').map(JSON.parse);
    const last = lines[lines.length - 1];
    assert.equal(last.indexOf('--json'), 2, `--json 不应被挪到尾部: ${JSON.stringify(last)}`);
  } finally {
    await b.stop();
    fs.rmSync(argvLog, { force: true });
  }
});

// ───────────── 3. 子进程输出 ─────────────

test('多字节 UTF-8 逐字节输出不被破坏', async () => {
  const b = await startBridge({ env: { STUB_MODE: 'multibyte' } });
  try {
    const body = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(body.ok, true, JSON.stringify(body));
    assert.equal(body.data.title, '任务：源文件索引 — αβγ 🧭');
    assert.ok(!JSON.stringify(body).includes('�'), '出现了替换字符');
  } finally {
    await b.stop();
  }
});

test('超大输出被挡住而不是撑爆内存', async () => {
  const b = await startBridge({ env: { STUB_MODE: 'huge' } });
  try {
    const body = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(body.ok, false);
    assert.equal(body.error.code, 'OutputTooLarge');
  } finally {
    await b.stop();
  }
});

test('超大请求体被拒', async () => {
  const res = await fetch(`${bridge.base}/api/context/compile`, {
    method: 'POST',
    headers: Object.assign({ 'content-type': 'application/json' }, GUARD),
    body: JSON.stringify({ work: 'A', intent: 'x'.repeat(200000) }),
  });
  assert.equal(res.status, 413);
  assert.equal((await res.json()).error.code, 'BodyTooLarge');
});

test('stderr 上的 JSON 错误能还原出 code', async () => {
  const b = await startBridge({ env: { STUB_MODE: 'stderrjson' } });
  try {
    const body = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(body.ok, false);
    assert.equal(body.error.code, 'SourceStale');
  } finally {
    await b.stop();
  }
});

// ───────────── 4. reindex 的语义 ─────────────

test('默认不允许 reindex', async () => {
  const body = await (
    await fetch(`${bridge.base}/api/source/reindex`, { method: 'POST', headers: GUARD })
  ).json();
  assert.equal(body.ok, false);
  assert.equal(body.error.code, 'ReindexNotAllowed');
});

test('--allow-reindex 之后可以跑', async () => {
  const b = await startBridge({ allowReindex: true });
  try {
    const body = await (
      await fetch(`${b.base}/api/source/reindex`, { method: 'POST', headers: GUARD })
    ).json();
    assert.equal(body.ok, true, JSON.stringify(body));
  } finally {
    await b.stop();
  }
});

// ───────────── 5. 演示模式 ─────────────

test('演示模式不执行任何 awr 命令', async () => {
  const argvLog = path.join(os.tmpdir(), `argv3-${Date.now()}.log`);
  const b = await startBridge({ demo: true, env: { STUB_ARGV_OUT: argvLog } });
  try {
    const health = await (await fetch(`${b.base}/api/health`, { headers: GUARD })).json();
    assert.equal(health.data.mode, 'demo');

    const status = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(status.error.code, 'DemoMode');

    assert.ok(!fs.existsSync(argvLog), '演示模式下不应有任何 awr 调用');
  } finally {
    await b.stop();
    fs.rmSync(argvLog, { force: true });
  }
});

// ───────────── 6. 复核提出的四个 P2 ─────────────

test('畸形请求行不会带走整个进程', async () => {
  const b = await startBridge();
  try {
    // `GET // HTTP/1.1` 会让 new URL('//', base) 抛出。
    const bad = await new Promise((resolve, reject) => {
      const sock = require('node:net').connect(b.port, '127.0.0.1', () => {
        sock.write('GET // HTTP/1.1\r\nHost: 127.0.0.1:' + b.port + '\r\n\r\n');
      });
      let text = '';
      sock.on('data', (d) => (text += d));
      sock.on('end', () => resolve(text));
      sock.on('error', reject);
      setTimeout(() => { sock.end(); resolve(text); }, 1500);
    });
    assert.ok(/HTTP\/1\.1 4\d\d/.test(bad), `期望 4xx，实际响应头：${bad.slice(0, 60)}`);

    // 关键断言：进程还活着，后续请求照常。
    const health = await fetch(`${b.base}/api/health`, { headers: GUARD });
    assert.equal(health.status, 200);
    assert.equal((await health.json()).ok, true);
  } finally {
    await b.stop();
  }
});

test('槽位按子进程释放，不按响应释放', async () => {
  // 写超时缩到 300ms，子进程活 30 秒：响应早就回了，子进程还在。
  const b = await startBridge({
    allowReindex: true,
    env: { STUB_MODE: 'slowwrite', AWR_INSPECTOR_WRITE_TIMEOUT_MS: '300' },
  });
  try {
    const hit = () =>
      fetch(`${b.base}/api/source/reindex`, { method: 'POST', headers: GUARD }).then((r) => r.json());

    const first = [];
    for (let i = 0; i < 4; i++) first.push(await hit());
    for (const r of first) {
      assert.equal(r.error.code, 'OutcomeUnknown', JSON.stringify(r));
    }

    // 四个子进程都还活着，第五个必须被挡下来。
    const fifth = await hit();
    assert.equal(fifth.error.code, 'BridgeBusy', JSON.stringify(fifth));
  } finally {
    await b.stop();
  }
});

test('写命令输出溢出不被 SIGKILL，结果报为未知', async () => {
  const b = await startBridge({
    allowReindex: true,
    env: { STUB_MODE: 'hugewrite' },
  });
  try {
    const r = await (
      await fetch(`${b.base}/api/source/reindex`, { method: 'POST', headers: GUARD })
    ).json();
    assert.equal(r.ok, false);
    // 不是 OutputTooLarge：写命令没被终止，成没成是未知的。
    assert.equal(r.error.code, 'OutcomeUnknown', JSON.stringify(r));
    assert.ok(!/终端里直接跑|重试/.test(r.error.message) || /不要直接重试/.test(r.error.message),
      '不该建议直接重跑一个结果未知的写操作');
  } finally {
    await b.stop();
  }
});

test('只读命令输出溢出仍然是 OutputTooLarge', async () => {
  const b = await startBridge({ env: { STUB_MODE: 'huge' } });
  try {
    const r = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(r.error.code, 'OutputTooLarge', JSON.stringify(r));
  } finally {
    await b.stop();
  }
});

// ───────────── 7. 前端：详情响应的代际守卫 ─────────────

test('迟到的详情响应不会覆盖当前选中项', () => {
  const { createGenerationGuard } = require('../public/app.js');
  const guard = createGenerationGuard();

  const a = guard.begin('A');
  const b = guard.begin('B');

  // B 先回：它是最新的，应当落地。
  assert.equal(guard.isCurrent(b), true);
  // A 后回：已经过期，必须丢掉。
  assert.equal(guard.isCurrent(a), false);
});

test('刷新会作废在途的详情请求', () => {
  const { createGenerationGuard } = require('../public/app.js');
  const guard = createGenerationGuard();

  const inflight = guard.begin('A');
  guard.invalidate();
  assert.equal(guard.isCurrent(inflight), false, '刷新后旧请求不得落地');

  // 同一个 key 的更早请求，在刷新后回来也不算数。
  const fresh = guard.begin('A');
  const older = { generation: fresh.generation - 1, key: 'A' };
  assert.equal(guard.isCurrent(older), false);
  assert.equal(guard.isCurrent(fresh), true);
});

test('退出码非 0 但带完整报告时，报告仍然交给前端', async () => {
  // `context compile` 判定上下文不完整时会退出 1，可 stdout 上的报告是完整的——
  // 那份诊断正是这时候最该看的东西，不能因为退出码就丢掉。
  const b = await startBridge({ env: { STUB_MODE: 'incomplete' } });
  try {
    const r = await (
      await fetch(`${b.base}/api/context/compile`, {
        method: 'POST',
        headers: Object.assign({ 'content-type': 'application/json' }, GUARD),
        body: JSON.stringify({ work: 'RECON-020', budget: 8000 }),
      })
    ).json();

    assert.equal(r.ok, false, '退出码非 0，如实报为失败');
    assert.equal(r.error.code, 'ContextIncomplete');
    assert.ok(r.data, '报告必须一并带上，否则完整性面板什么也显示不了');
    assert.equal(r.data.completeness.status, 'CONTEXT INCOMPLETE');
    assert.equal(r.data.completeness.rules_complete, false);
    assert.ok(r.data.work_context.rendered_context.length > 0);
  } finally {
    await b.stop();
  }
});

test('纯错误响应不会被误当成报告', async () => {
  // 只有 code/message 的错误壳子不算载荷。
  const b = await startBridge({ env: { STUB_MODE: 'stderrjson' } });
  try {
    const r = await (await fetch(`${b.base}/api/status`, { headers: GUARD })).json();
    assert.equal(r.ok, false);
    assert.equal(r.error.code, 'SourceStale');
    assert.ok(!r.data, '错误壳子不该被当成数据交给前端');
  } finally {
    await b.stop();
  }
});

// ───────────── 8. issue #63：预算上限 ─────────────

test('budget 放得到 AWR 的上限，界面不该再卡在 16000', async () => {
  // issue #63 第 1 条：下拉框封顶 16000。
  // 真实上限是 100000（crates/awr-context/src/budget.rs），不是 issue 里说的 200000——
  // 那个数是桥接自己的旧常量。
  const r = await (
    await fetch(`${bridge.base}/api/context/compile`, {
      method: 'POST',
      headers: Object.assign({ 'content-type': 'application/json' }, GUARD),
      body: JSON.stringify({ work: 'RECON-001', budget: 100000 }),
    })
  ).json();
  assert.ok(r.command.includes('--budget 100000'), `预算没透传: ${r.command}`);
});

test('超过 AWR 上限的 budget 直接拒绝，不悄悄换成默认值', async () => {
  const r = await (
    await fetch(`${bridge.base}/api/context/compile`, {
      method: 'POST',
      headers: Object.assign({ 'content-type': 'application/json' }, GUARD),
      body: JSON.stringify({ work: 'RECON-001', budget: 100001 }),
    })
  ).json();
  // 之前是不带 --budget 让 AWR 用默认 5000——一个 105000 的请求会以 5000 跑一遍再失败。
  // 现在明确拒，错误里写清范围。
  assert.equal(r.ok, false);
  assert.equal(r.error.code, 'BadRequest');
  assert.ok(/100000/.test(r.error.message), `错误信息要给出范围: ${r.error.message}`);
  assert.ok(!r.command, '不该起子进程');
});

// ───────────── 9. shell:false 路径安全 ─────────────

test('--project 含空格的路径正常工作', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'proj '));
  try {
    const b = await startBridge({ project: tmpDir });
    try {
      const r = await fetch(`${b.base}/api/status`, { headers: GUARD });
      const body = await r.json();
      assert.equal(body.ok, true, `含空格的项目路径应正常工作, got: ${JSON.stringify(body)}`);
    } finally {
      await b.stop();
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('search preserves shell metacharacters as one literal argument', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'awr-argv-'));
  const argvLog = path.join(dir, 'argv.jsonl');
  const text = 'literal & | ^ > < %PATH% "quoted words"';
  let b;
  try {
    b = await startBridge({ env: { STUB_ARGV_OUT: argvLog } });
    const res = await fetch(`${b.base}/api/search?text=${encodeURIComponent(text)}`, {
      headers: GUARD,
    });
    const body = await res.json();
    assert.equal(res.status, 200);
    assert.equal(body.ok, true, JSON.stringify(body));
    assert.equal(body.data.query.text, text);

    const calls = fs.readFileSync(argvLog, 'utf8').trim().split('\n').map(JSON.parse);
    const searches = calls.filter((args) => args.includes('search'));
    assert.equal(searches.length, 1, 'Exactly one search command must run');
    const args = searches[0];
    const separator = args.indexOf('--');
    assert.ok(separator >= 0, 'Search text must follow the option terminator');
    assert.deepEqual(args.slice(separator + 1), [text]);
  } finally {
    if (b) await b.stop();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('分页参数传给 status，非法页大小和偏移不执行命令', async () => {
  const result = await (await fetch(`${bridge.base}/api/work-page?queue=ready&offset=10&limit=20`, {headers:GUARD})).json();
  assert.match(result.command, /--queue ready --offset 10 --page-size 20/);
  for (const query of ['queue=other', 'offset=-1', 'offset=1.5', 'limit=0', 'limit=101']) {
    const invalid=await (await fetch(`${bridge.base}/api/work-page?${query}`, {headers:GUARD})).json();
    assert.equal(invalid.ok,false); assert.equal(invalid.error.code,'BadRequest');
  }
});
