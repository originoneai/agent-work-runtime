#!/usr/bin/env node
/**
 * AWR Console —— 本地桥接进程
 *
 * 做的事情只有一件：把一次 HTTP 请求翻译成一条 `awr --json ...` 命令，
 * 把 AWR 原样吐出的 JSON 原样转发给浏览器。它不解释、不改写、不缓存。
 *
 * 只绑定 127.0.0.1，只允许白名单内的子命令，参数不拼 shell。
 *
 * 用法：
 *   node server.js --project /abs/path/to/project [--port 7381] [--demo]
 */

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn, execFile } = require('child_process');

// ───────────────────────── 参数 ─────────────────────────

function parseArgs(argv) {
  const out = { project: process.cwd(), port: 7381, demo: false, open: true };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--project' || a === '-p') out.project = path.resolve(argv[++i] || '.');
    else if (a === '--port') out.port = Number(argv[++i]) || out.port;
    else if (a === '--demo') out.demo = true;
    else if (a === '--no-open') out.open = false;
    else if (a === '--help' || a === '-h') {
      console.log('用法: node server.js --project <项目目录> [--port 7381] [--demo] [--no-open]');
      process.exit(0);
    }
  }
  return out;
}

const ARGS = parseArgs(process.argv.slice(2));
const PUBLIC_DIR = path.join(__dirname, 'public');

// ───────────────────────── awr 探测 ─────────────────────────

// 桥接启动时探一次：awr 在不在、项目初始化没有。
// 探不到就整个进演示模式，前端会显示一条说明横幅，而不是一片红色报错。
const runtime = {
  mode: ARGS.demo ? 'demo' : 'unknown', // 'live' | 'demo'
  awrVersion: null,
  project: ARGS.project,
  reason: ARGS.demo ? '启动时带了 --demo 参数' : null,
  // `--json` 放在哪：AWR 文档里两种写法都出现过
  // (`awr --json work edit ...` 和 `awr ... intake inspect --json`)。
  // 先按全局位置试，被拒就改成放在子命令后面，探测结果记在这里。
  jsonFlagPosition: 'global',
};

function detectAwr() {
  return new Promise((resolve) => {
    if (ARGS.demo) return resolve();
    execFile('awr', ['--version'], { timeout: 8000 }, (err, stdout) => {
      if (err) {
        runtime.mode = 'demo';
        runtime.reason = '没有找到 awr 命令。装好之后重启本进程即可看到真实数据。';
        return resolve();
      }
      runtime.awrVersion = String(stdout).trim();
      runtime.mode = 'live';
      resolve();
    });
  });
}

// ───────────────────────── 执行 awr ─────────────────────────

// 白名单。键是前端能请求的动作名，值是这个动作允许的固定子命令。
// 前端传不了任意命令，只能在这张表里挑一个，再补上经过校验的参数。
const COMMANDS = {
  status: { argv: ['status'], write: false },
  ready: { argv: ['ready'], write: false },
  workShow: { argv: ['work', 'show'], write: false },
  search: { argv: ['search'], write: false },
  intakeInspect: { argv: ['intake', 'inspect'], write: false },
  contextCompile: { argv: ['context', 'compile'], write: false },
  sourceReindex: { argv: ['source', 'reindex'], write: true },
};

// 参数值的合法形状。AWR 的 key 是 EXAMPLE-001、goal#demo 这类东西。
const SAFE_VALUE = /^[A-Za-z0-9_@.:#/\\ -]{1,200}$/;

function safe(value) {
  const s = String(value == null ? '' : value);
  return SAFE_VALUE.test(s) ? s : null;
}

function buildArgv(commandKey, extra) {
  const spec = COMMANDS[commandKey];
  if (!spec) throw new Error(`不允许的命令: ${commandKey}`);
  const base = ['--project', runtime.project];
  if (runtime.jsonFlagPosition === 'global') base.push('--json');
  const argv = base.concat(spec.argv, extra || []);
  if (runtime.jsonFlagPosition === 'trailing') argv.push('--json');
  return argv;
}

function execAwr(argv) {
  return new Promise((resolve) => {
    const child = spawn('awr', argv, { shell: false });
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => child.kill('SIGKILL'), 60000);

    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('error', (err) => {
      clearTimeout(timer);
      resolve({ code: -1, stdout: '', stderr: String(err.message) });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr });
    });
  });
}

/**
 * 跑一条命令，返回给前端的统一信封。
 * 无论成败都带上 `command`：界面上那条“可以复制去终端跑”的命令就是它。
 */
async function runCommand(commandKey, extra) {
  let argv = buildArgv(commandKey, extra);
  let result = await execAwr(argv);

  // --json 位置探测：全局位置被拒时换到子命令后面再试一次。
  if (result.code !== 0 && runtime.jsonFlagPosition === 'global' && looksLikeUnknownFlag(result.stderr)) {
    runtime.jsonFlagPosition = 'trailing';
    argv = buildArgv(commandKey, extra);
    result = await execAwr(argv);
  }

  const command = 'awr ' + argv.map(quoteForDisplay).join(' ');

  if (result.code === -1) {
    return { ok: false, command, error: { code: 'BridgeSpawnFailed', message: result.stderr } };
  }

  const parsed = tryParseJson(result.stdout);

  if (result.code !== 0) {
    // AWR 的错误也是 JSON，带 code 和 message，但它可能走 stdout 也可能走 stderr。
    // 两边都试，能解析出 code 就原样交给前端——界面上要显示的就是这个 code。
    const errJson = (parsed && (parsed.code || parsed.error) ? parsed : null)
      || tryParseJson(result.stderr);
    const domain = errJson && (errJson.error || errJson);
    return {
      ok: false,
      command,
      exitCode: result.code,
      error: domain && domain.code
        ? domain
        : { code: 'CommandFailed', message: (result.stderr || result.stdout || '').trim() },
      raw: errJson || null,
    };
  }

  if (!parsed) {
    return {
      ok: false,
      command,
      error: { code: 'NotJson', message: 'awr 返回的不是 JSON。原始输出见 raw。' },
      raw: result.stdout.slice(0, 20000),
    };
  }

  return { ok: true, command, data: parsed };
}

function looksLikeUnknownFlag(stderr) {
  const s = String(stderr).toLowerCase();
  return s.includes('unexpected argument') || s.includes('unknown') || s.includes('unrecognized');
}

function tryParseJson(text) {
  const t = String(text || '').trim();
  if (!t) return null;
  try {
    return JSON.parse(t);
  } catch (_) {
    // 有些命令会先打印几行人类可读的文本再打印 JSON，取第一个 { 起的部分再试。
    const i = t.indexOf('{');
    if (i > 0) {
      try {
        return JSON.parse(t.slice(i));
      } catch (_) {
        return null;
      }
    }
    return null;
  }
}

function quoteForDisplay(arg) {
  return /[^A-Za-z0-9_@.:#/=-]/.test(arg) ? `'${arg.replace(/'/g, `'\\''`)}'` : arg;
}

// ───────────────────────── 路由 ─────────────────────────

const routes = {
  'GET /api/health': async () => ({
    ok: true,
    data: {
      mode: runtime.mode,
      awrVersion: runtime.awrVersion,
      project: runtime.project,
      reason: runtime.reason,
      bridgeVersion: '1.0.0',
    },
  }),

  'GET /api/status': async (url) => {
    const extra = [];
    const view = safe(url.searchParams.get('view'));
    if (view === 'full' || view === 'summary') extra.push('--view', view);
    return runCommand('status', extra);
  },

  'GET /api/ready': async (url) => {
    const extra = [];
    const limit = Number(url.searchParams.get('limit'));
    if (Number.isInteger(limit) && limit >= 1 && limit <= 100) extra.push('--limit', String(limit));
    return runCommand('ready', extra);
  },

  'GET /api/work': async (url) => {
    const key = safe(url.searchParams.get('key'));
    if (!key) return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 key 参数' } };
    return runCommand('workShow', [key]);
  },

  'GET /api/search': async (url) => {
    const text = safe(url.searchParams.get('text'));
    if (!text) return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 text 参数' } };
    const extra = ['--text', text];
    const limit = Number(url.searchParams.get('limit'));
    if (Number.isInteger(limit) && limit >= 1 && limit <= 100) extra.push('--limit', String(limit));
    return runCommand('search', extra);
  },

  'GET /api/sources': async () => runCommand('intakeInspect', []),

  'POST /api/context/compile': async (_url, body) => {
    const extra = [];
    const work = safe(body.work);
    const goal = safe(body.goal);
    const branch = safe(body.branch);
    const intent = safe(body.intent);
    if (!work) return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 work' } };
    extra.push('--work', work);
    if (goal) extra.push('--goal', goal);
    const budget = Number(body.budget);
    if (Number.isInteger(budget) && budget >= 500 && budget <= 200000) extra.push('--budget', String(budget));
    if (branch) extra.push('--branch', branch);
    if (intent) extra.push('--intent', intent);
    return runCommand('contextCompile', extra);
  },

  'POST /api/source/reindex': async () => runCommand('sourceReindex', []),
};

// ───────────────────────── HTTP ─────────────────────────

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
};

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
  });
  res.end(body);
}

function serveStatic(req, res, pathname) {
  const rel = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '');
  const file = path.join(PUBLIC_DIR, rel);
  // 目录穿越防护：解析后的路径必须还在 public 里面。
  if (!file.startsWith(PUBLIC_DIR + path.sep) && file !== path.join(PUBLIC_DIR, 'index.html')) {
    res.writeHead(403).end('forbidden');
    return;
  }
  fs.readFile(file, (err, buf) => {
    if (err) {
      res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      res.end('404');
      return;
    }
    res.writeHead(200, {
      'content-type': MIME[path.extname(file)] || 'application/octet-stream',
      // 不缓存：改了 public/ 里的文件，刷新页面就能看到，不用清缓存。
      'cache-control': 'no-store',
    });
    res.end(buf);
  });
}

function readBody(req) {
  return new Promise((resolve) => {
    let raw = '';
    req.on('data', (d) => {
      raw += d;
      if (raw.length > 1e6) req.destroy();
    });
    req.on('end', () => {
      try {
        resolve(raw ? JSON.parse(raw) : {});
      } catch (_) {
        resolve({});
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1');
  const key = `${req.method} ${url.pathname}`;

  if (url.pathname.startsWith('/api/')) {
    const handler = routes[key];
    if (!handler) return sendJson(res, 404, { ok: false, error: { code: 'NoRoute', message: key } });

    // 演示模式下除了 /api/health 一律不执行命令，前端自己用内置样本数据。
    if (runtime.mode === 'demo' && url.pathname !== '/api/health') {
      return sendJson(res, 200, {
        ok: false,
        error: { code: 'DemoMode', message: runtime.reason || '当前是演示模式，没有连接真实项目。' },
      });
    }

    try {
      const body = req.method === 'POST' ? await readBody(req) : null;
      const result = await handler(url, body);
      return sendJson(res, 200, result);
    } catch (err) {
      return sendJson(res, 200, { ok: false, error: { code: 'BridgeError', message: String(err.message) } });
    }
  }

  serveStatic(req, res, url.pathname);
});

// ───────────────────────── 启动 ─────────────────────────

detectAwr().then(() => {
  server.listen(ARGS.port, '127.0.0.1', () => {
    const addr = `http://127.0.0.1:${ARGS.port}`;
    console.log('');
    console.log('  AWR Console 已启动');
    console.log('  ─────────────────────────────────────────');
    console.log(`  地址    ${addr}`);
    console.log(`  项目    ${runtime.project}`);
    if (runtime.mode === 'live') {
      console.log(`  模式    真实数据（${runtime.awrVersion || 'awr'}）`);
    } else {
      console.log('  模式    演示模式');
      console.log(`  原因    ${runtime.reason}`);
    }
    console.log('  ─────────────────────────────────────────');
    console.log('  按 Ctrl+C 停止');
    console.log('');
    if (ARGS.open) {
      const opener = process.platform === 'darwin' ? 'open'
        : process.platform === 'win32' ? 'explorer' : 'xdg-open';
      spawn(opener, [addr], { stdio: 'ignore', detached: true }).on('error', () => {});
    }
  });

  server.on('error', (err) => {
    if (err.code === 'EADDRINUSE') {
      console.error(`端口 ${ARGS.port} 已被占用。换一个：node server.js --port ${ARGS.port + 1}`);
    } else {
      console.error(err.message);
    }
    process.exit(1);
  });
});
