#!/usr/bin/env node
/**
 * AWR Inspector —— 本地桥接进程
 *
 * 做的事情只有一件：把一次 HTTP 请求翻译成一条 `awr --json` 命令，
 * 把 AWR 原样吐出的 JSON 原样转发给浏览器。它不解释、不改写、不缓存。
 *
 * 绑定回环地址还不够：浏览器里的任意页面都能向 127.0.0.1 发请求。
 * 所以 /api/* 还有一层请求来源边界，见 guardRequest()。
 *
 * 用法：
 *   node server.js --project /abs/path/to/project [--port 7381] [--demo] [--allow-reindex]
 */

'use strict';

const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn, execFile } = require('child_process');

// ───────────────────────── 上限 ─────────────────────────

/** 只给测试用的数值覆盖；没设就用默认。 */
function envInt(name, fallback) {
  const v = Number(process.env[name]);
  return Number.isInteger(v) && v > 0 ? v : fallback;
}

const LIMITS = {
  stdoutBytes: 8 * 1024 * 1024,   // 单条命令的 stdout 上限
  stderrBytes: 1 * 1024 * 1024,
  requestBytes: 64 * 1024,        // 请求体上限
  concurrent: envInt('AWR_INSPECTOR_CONCURRENT', 4),          // 同时在跑的 awr 子进程数
  readTimeoutMs: envInt('AWR_INSPECTOR_READ_TIMEOUT_MS', 60000),   // 查询命令的超时
  writeTimeoutMs: envInt('AWR_INSPECTOR_WRITE_TIMEOUT_MS', 120000), // 写命令（reindex）的超时
};

// ───────────────────────── 参数 ─────────────────────────

function parseArgs(argv) {
  const out = {
    project: process.cwd(),
    port: 7381,
    demo: false,
    open: true,
    allowReindex: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--project' || a === '-p') out.project = path.resolve(argv[++i] || '.');
    else if (a === '--port') out.port = Number(argv[++i]) || out.port;
    else if (a === '--demo') out.demo = true;
    else if (a === '--no-open') out.open = false;
    else if (a === '--allow-reindex') out.allowReindex = true;
    else if (a === '--help' || a === '-h') {
      console.log([
        '用法: node server.js [选项]',
        '',
        '  --project <目录>   要查看的 AWR 项目，默认当前目录',
        '  --port <端口>      默认 7381',
        '  --demo             强制演示模式，不执行任何真实命令',
        '  --allow-reindex    允许从界面触发 `source reindex`（默认不允许）',
        '  --no-open          不自动打开浏览器',
      ].join('\n'));
      process.exit(0);
    }
  }
  return out;
}

const ARGS = parseArgs(process.argv.slice(2));
const PUBLIC_DIR = path.join(__dirname, 'public');

// ───────────────────────── awr 路径解析 ─────────────────────────

/**
 * 解析 .cmd 文件，提取 Node.js 入口路径。
 * npm 的 .cmd 包装器格式：`"%_prog%" "<entry_point>" %*`
 * 测试 stub 的 .cmd 格式：`"node" "<entry_point>" %*`
 * 返回展开 %dp0% 后的绝对路径，找不到返回 null。
 */
function parseCmdEntryPoint(cmdPath) {
  try {
    const content = fs.readFileSync(cmdPath, 'utf8');
    const match = content.match(/"[^"]+"\s+"([^"]+)"\s+%\*/);
    if (match) {
      let entryPoint = match[1].replace(/%dp0%/g, path.dirname(cmdPath));
      if (fs.existsSync(entryPoint)) return entryPoint;
    }
  } catch {}
  return null;
}

/**
 * 通过 PATH × PATHEXT 解析 awr 的完整路径，全程 shell: false。
 *
 * 返回 { exe, needsNode }：
 *   - exe: 可执行文件路径（.exe / .cmd / 扩展名空的 POSIX 脚本）
 *   - needsNode: true 时 exe 是 .cmd/.bat 包装器，spawn 时需用 process.execPath 作为 command，
 *     exe 作为第一个参数；false 时 exe 可直接 spawn。
 *   两者均找不到时 exe=null，调用方进 demo mode。
 */
function resolveAwr() {
  const sep = process.platform === 'win32' ? ';' : ':';
  const exts = process.platform === 'win32'
    ? (process.env.PATHEXT || '.COM;.EXE;.BAT;.CMD').split(';').map(e => e.toUpperCase())
    : [''];
  const dirs = (process.env.PATH || '').split(sep);

  for (const dir of dirs) {
    for (const ext of exts) {
      const candidate = path.join(dir, 'awr' + ext);
      try {
        const st = fs.statSync(candidate, { throwIfNoEntry: false });
        if (st && st.isFile()) {
          const extUpper = path.extname(candidate).toUpperCase();
          // .EXE 或无扩展名（POSIX 脚本）可直接 spawn
          if (extUpper === '.EXE' || extUpper === '') {
            return { exe: candidate, needsNode: false };
          }
          // .CMD/.BAT 是 npm 包装器，需要解析出 Node 入口
          if (extUpper === '.CMD' || extUpper === '.BAT') {
            const entryPoint = parseCmdEntryPoint(candidate);
            if (entryPoint) return { exe: candidate, needsNode: true, entryPoint };
          }
        }
      } catch {}
    }
  }
  return { exe: null, needsNode: false };
}

const AWR_RESOLVED = resolveAwr();

// ───────────────────────── awr 探测 ─────────────────────────

const runtime = {
  mode: ARGS.demo ? 'demo' : 'unknown', // 'live' | 'demo'
  awrVersion: null,
  project: ARGS.project,
  reason: ARGS.demo ? '启动时带了 --demo 参数' : null,
  allowReindex: ARGS.allowReindex,
  // `--json` 放全局位置。只有当 AWR 明确说不认识 `--json` 时才改放尾部；
  // 别的参数报错不能动这个开关（那会让一次坏请求污染整个进程）。
  jsonFlagPosition: 'global',
  running: 0,
};

function detectAwr() {
  return new Promise((resolve) => {
    if (ARGS.demo) return resolve();
    if (!AWR_RESOLVED.exe) {
      runtime.mode = 'demo';
      runtime.reason = '没有找到 awr 命令。装好之后重启本进程即可看到真实数据。';
      return resolve();
    }
    const args = AWR_RESOLVED.needsNode ? [AWR_RESOLVED.entryPoint, '--version'] : ['--version'];
    const cmd = AWR_RESOLVED.needsNode ? process.execPath : AWR_RESOLVED.exe;
    execFile(cmd, args, { timeout: 8000 }, (err, stdout) => {
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

// ───────────────────────── 请求来源边界 ─────────────────────────

const ALLOWED_HOSTS = new Set([
  `127.0.0.1:${ARGS.port}`,
  `localhost:${ARGS.port}`,
  `[::1]:${ARGS.port}`,
]);
const ALLOWED_ORIGINS = new Set([
  `http://127.0.0.1:${ARGS.port}`,
  `http://localhost:${ARGS.port}`,
  `http://[::1]:${ARGS.port}`,
]);

/** 状态变更请求必须带这个头。第三方页面发不出自定义头，除非先过 CORS 预检——我们不给预检放行。 */
const GUARD_HEADER = 'x-awr-inspector';

/**
 * 判断一个 /api/* 请求是不是真的来自本机这个页面。
 * 返回 null 表示放行，否则返回要回给调用方的错误。
 */
function guardRequest(req) {
  // 1) Host：挡 DNS rebinding。攻击者把域名解析到 127.0.0.1，Host 仍是他的域名。
  const host = String(req.headers.host || '').toLowerCase();
  if (!ALLOWED_HOSTS.has(host)) {
    return { code: 'ForbiddenHost', message: `不接受的 Host: ${host || '(空)'}` };
  }

  // 2) Origin：带了就必须是本机这个源。'null' 也不放行（沙箱 iframe、file:// 都会发它）。
  const origin = req.headers.origin;
  if (origin !== undefined && !ALLOWED_ORIGINS.has(String(origin))) {
    return { code: 'ForbiddenOrigin', message: `不接受的 Origin: ${origin}` };
  }

  // 3) Sec-Fetch-Site：浏览器自己标的，页面改不了。
  //    同源请求是 same-origin；地址栏直接打开是 none。其余一律拒。
  const site = req.headers['sec-fetch-site'];
  if (site !== undefined && site !== 'same-origin' && site !== 'none') {
    return { code: 'ForbiddenSite', message: `不接受的 Sec-Fetch-Site: ${site}` };
  }

  // 4) 状态变更请求要带自定义头。表单跨站 POST 发不出它。
  if (req.method !== 'GET' && req.headers[GUARD_HEADER] !== '1') {
    return {
      code: 'MissingGuardHeader',
      message: `状态变更请求必须带 ${GUARD_HEADER}: 1 请求头`,
    };
  }

  return null;
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

// 每个字段一套校验，按它实际承载什么来定，不用一条粗放的 ASCII 正则一刀切。
// 注入风险已经由「参数数组 + 不走 shell」消掉了，这里管的是「值合不合理」。

/** AWR 的 key：EXAMPLE-001、goal#demo、plan#intake 这类。 */
const KEY_RE = /^[A-Za-z0-9_.:#/-]{1,200}$/;

/** 分支名。 */
const BRANCH_RE = /^[A-Za-z0-9_./-]{1,200}$/;

function asKey(value) {
  const s = String(value == null ? '' : value);
  return KEY_RE.test(s) ? s : null;
}

function asBranch(value) {
  const s = String(value == null ? '' : value);
  return BRANCH_RE.test(s) ? s : null;
}

/**
 * 自由文本（搜索词、intent）。允许 Unicode——中文搜索是正当需求。
 * 只挡控制字符和 NUL，并限长。
 */
function asText(value, maxLength) {
  const s = String(value == null ? '' : value);
  if (!s || s.length > (maxLength || 500)) return null;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    // 控制字符（含 NUL）一律不收；制表、换行、回车也不该出现在这类单行参数里。
    if (c < 0x20 || c === 0x7f) return null;
  }
  return s;
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

/**
 * 跑一条 awr。
 *
 * stdout/stderr 按 Buffer 收集，跑完再整体解码——按块解码会把一个多字节
 * UTF-8 字符劈成两半，拼回来就是 U+FFFD。
 *
 * 查询可能刷新 SQLite 投影，并非运行态只读。查询超时会请求终止进程；
 * 显式 `source reindex` 超时保留子进程并报告结果未知。
 */
function execAwr(argv, opts) {
  const write = Boolean(opts && opts.write);
  const timeoutMs = write ? LIMITS.writeTimeoutMs : LIMITS.readTimeoutMs;

  return new Promise((resolve) => {
    const cmd = AWR_RESOLVED.needsNode ? process.execPath : AWR_RESOLVED.exe;
    const args = AWR_RESOLVED.needsNode ? [AWR_RESOLVED.entryPoint, ...argv] : argv;
    const child = spawn(cmd, args, { stdio: 'pipe', windowsHide: true });

    // 槽位跟着子进程走，不跟着 HTTP 响应走。超时时我们会先回响应，
    // 但子进程还活着——那个槽必须留到它真的退出为止，否则上限形同虚设。
    runtime.running += 1;
    let released = false;
    const release = () => {
      if (released) return;
      released = true;
      runtime.running -= 1;
    };

    const out = [];
    const err = [];
    let outBytes = 0;
    let errBytes = 0;
    let truncated = false;
    let settled = false;

    const finish = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(Object.assign({ truncated, write }, result));
    };

    const timer = setTimeout(() => {
      if (write) {
        // 不杀。把子进程放掉，让它自己跑完；结果是未知的，如实说。
        // 槽位不在这里释放——等 close 事件。
        finish({ code: null, timedOut: true, outcomeUnknown: true, stdout: '', stderr: '' });
      } else {
        // 查询命令：先礼后兵，SIGTERM 给 5 秒，再 SIGKILL。
        child.kill('SIGTERM');
        const hard = setTimeout(() => child.kill('SIGKILL'), 5000);
        hard.unref();
        finish({ code: null, timedOut: true, outcomeUnknown: false, stdout: '', stderr: '' });
      }
    }, timeoutMs);
    // 超时定时器不该拖住进程退出。
    timer.unref();

    child.stdout.on('data', (chunk) => {
      outBytes += chunk.length;
      if (outBytes > LIMITS.stdoutBytes) {
        truncated = true;
        // 查询命令可以杀。写命令不行——杀掉一个正在改状态的 reindex
        // 会留下不知道成没成的状态，而输出太大并不是终止它的理由。
        // 继续读，只是把超出的部分丢掉。
        if (!write) child.kill('SIGKILL');
        return;
      }
      out.push(chunk);
    });
    child.stderr.on('data', (chunk) => {
      errBytes += chunk.length;
      if (errBytes > LIMITS.stderrBytes) {
        truncated = true;
        return;
      }
      err.push(chunk);
    });

    child.on('error', (e) => {
      release();
      finish({ code: -1, stdout: '', stderr: String(e.message) });
    });
    child.on('close', (code) => {
      release();
      finish({
        code,
        stdout: Buffer.concat(out).toString('utf8'),
        stderr: Buffer.concat(err).toString('utf8'),
      });
    });
  });
}

/**
 * 跑一条命令，返回给前端的统一信封。
 * 无论成败都带上 `command`：界面上那条「可以复制去终端跑」的命令就是它。
 */
async function runCommand(commandKey, extra) {
  const spec = COMMANDS[commandKey];

  // 槽位由 execAwr 按子进程生命周期占用与释放，这里只做准入判断。
  if (runtime.running >= LIMITS.concurrent) {
    return {
      ok: false,
      command: null,
      error: {
        code: 'BridgeBusy',
        message: '同时在跑的 awr 子进程已达上限，等其中一个结束再试。',
      },
    };
  }

  {
    let argv = buildArgv(commandKey, extra);
    let result = await execAwr(argv, spec);

    // --json 位置探测：只有当 AWR 明确说不认识 `--json` 时才换位置重试。
    if (
      result.code !== 0 &&
      runtime.jsonFlagPosition === 'global' &&
      mentionsUnknownJsonFlag(result.stderr, result.stdout)
    ) {
      runtime.jsonFlagPosition = 'trailing';
      argv = buildArgv(commandKey, extra);
      result = await execAwr(argv, spec);
    }

    const command = 'awr ' + argv.map(quoteForDisplay).join(' ');

    if (result.timedOut) {
      return {
        ok: false,
        command,
        error: result.outcomeUnknown
          ? {
              code: 'OutcomeUnknown',
              message:
                '命令超时了，但它没有被终止，可能已经生效，也可能没有。' +
                '先用 awr 查一下当前状态再决定下一步，不要直接重试。',
            }
          : {
              code: 'BridgeTimeout',
              message: '查询超时，已请求终止。查询可能已刷新本地投影；请检查当前状态后再决定是否重试。',
            },
      };
    }

    if (result.code === -1) {
      return { ok: false, command, error: { code: 'BridgeSpawnFailed', message: result.stderr } };
    }

    if (result.truncated) {
      // 写命令没被终止，只是输出没收全——它成没成是未知的，别叫人直接重跑。
      return spec.write
        ? {
            ok: false,
            command,
            error: {
              code: 'OutcomeUnknown',
              message:
                `输出超过了 ${LIMITS.stdoutBytes} 字节上限，没有收全。命令本身没有被终止，` +
                '可能已经生效。先用 awr 查一下当前状态再决定下一步，不要直接重试。',
            },
          }
        : {
            ok: false,
            command,
            error: {
              code: 'OutputTooLarge',
              message: `awr 的输出超过了 ${LIMITS.stdoutBytes} 字节上限。请在终端里直接跑这条命令。`,
            },
          };
    }

    const parsed = tryParseJson(result.stdout);

    if (result.code !== 0) {
      // AWR 的错误也是 JSON，带 code 和 message，但它可能走 stdout 也可能走 stderr。
      const errJson =
        (parsed && (parsed.code || parsed.error) ? parsed : null) || tryParseJson(result.stderr);
      const domain = errJson && (errJson.error || errJson);

      // 退出码非 0 不等于「没有结果」。
      // 比如 `context compile` 在上下文不完整时会退出 1，但 stdout 上照样给出
      // 完整的报告——完整性的各个维度、issues、证据缺口全在里面，那正是这时候
      // 最需要看的东西。能解析出真正的载荷就一并带上，让界面自己决定怎么呈现。
      const payload = carriesPayload(parsed) ? parsed : null;

      return {
        ok: false,
        command,
        exitCode: result.code,
        error: domain && domain.code
          ? domain
          : { code: 'CommandFailed', message: (result.stderr || result.stdout || '').trim() },
        data: payload,
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
}

/**
 * 判断一个解析出来的 JSON 是不是真的载荷，而不只是一个错误壳子。
 * 只有 code / message / error / details 这类字段的，是错误本身，不是结果。
 */
function carriesPayload(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const shell = ['code', 'message', 'error', 'details', 'ok'];
  return Object.keys(value).some((k) => shell.indexOf(k) < 0);
}

/** 只认「不认识 --json」这一种情况，别的参数报错不算。 */
function mentionsUnknownJsonFlag(stderr, stdout) {
  const s = (String(stderr) + String(stdout)).toLowerCase();
  if (!s.includes('--json')) return false;
  return (
    s.includes('unexpected argument') || s.includes('unknown') || s.includes('unrecognized')
  );
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
  return /[^A-Za-z0-9_@.:#/=-]/.test(arg) ? `'${String(arg).replace(/'/g, `'\\''`)}'` : arg;
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
      allowReindex: runtime.allowReindex,
      bridgeVersion: '1.1.0',
    },
  }),

  'GET /api/status': async (url) => {
    const extra = [];
    const view = asKey(url.searchParams.get('view'));
    if (view === 'full' || view === 'summary') extra.push('--view', view);
    return runCommand('status', extra);
  },

  'GET /api/work-page': async (url) => {
    const queue = url.searchParams.get('queue') || 'all';
    const offset = Number(url.searchParams.get('offset') || '0');
    const limit = Number(url.searchParams.get('limit') || '10');
    if (!['all', 'current', 'ready', 'waiting', 'blocked'].includes(queue) ||
        !Number.isSafeInteger(offset) || offset < 0 ||
        !Number.isInteger(limit) || limit < 1 || limit > 100) {
      return { ok: false, error: { code: 'BadRequest', message: '分页参数无效' } };
    }
    return runCommand('status', ['--queue', queue, '--offset', String(offset), '--page-size', String(limit)]);
  },

  'GET /api/ready': async (url) => {
    const extra = [];
    const limit = Number(url.searchParams.get('limit'));
    if (Number.isInteger(limit) && limit >= 1 && limit <= 100) extra.push('--limit', String(limit));
    return runCommand('ready', extra);
  },

  'GET /api/work': async (url) => {
    const key = asKey(url.searchParams.get('key'));
    if (!key) return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 key 参数' } };
    return runCommand('workShow', [key]);
  },

  'GET /api/search': async (url) => {
    // AWR 0.4.0 的签名是 `awr search [OPTIONS] [TEXT]`——文本是位置参数，不是 --text。
    const text = asText(url.searchParams.get('text'), 200);
    if (!text) {
      return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 text 参数' } };
    }
    const extra = [];
    const limit = Number(url.searchParams.get('limit'));
    if (Number.isInteger(limit) && limit >= 1 && limit <= 100) extra.push('--limit', String(limit));
    // `--` 之后是位置参数，这样以 `-` 开头的搜索词也不会被当成选项。
    extra.push('--', text);
    return runCommand('search', extra);
  },

  'GET /api/sources': async () => runCommand('intakeInspect', []),

  'POST /api/context/compile': async (_url, body) => {
    const extra = [];
    const work = asKey(body.work);
    if (!work) return { ok: false, error: { code: 'BadRequest', message: '缺少合法的 work' } };
    extra.push('--work', work);

    const goal = asKey(body.goal);
    if (goal) extra.push('--goal', goal);

    // 上限跟着 AWR 走：crates/awr-context/src/budget.rs 里是 1..100000。
    // 超限就明确拒绝。之前是悄悄不传 --budget 让 AWR 用默认的 5000——
    // 结果一个 105000 的请求会以 5000 跑一遍再失败，谁也看不懂发生了什么。
    if (body.budget !== undefined && body.budget !== null && body.budget !== '') {
      const budget = Number(body.budget);
      if (!Number.isInteger(budget) || budget < 500 || budget > 100000) {
        return {
          ok: false,
          error: { code: 'BadRequest', message: 'budget 必须是 500 到 100000 之间的整数' },
        };
      }
      extra.push('--budget', String(budget));
    }

    const branch = asBranch(body.branch);
    if (branch) extra.push('--branch', branch);

    const intent = asText(body.intent, 500);
    if (intent) extra.push('--intent', intent);

    return runCommand('contextCompile', extra);
  },

  'POST /api/source/reindex': async () => {
    if (!runtime.allowReindex) {
      return {
        ok: false,
        error: {
          code: 'ReindexNotAllowed',
          message: '重新索引默认是关的。要开启，用 --allow-reindex 重启本进程。',
        },
      };
    }
    return runCommand('sourceReindex', []);
  },
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

// 页面只许加载自己的东西。没有外部字体、没有内联脚本、没有外连。
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self'",
  "connect-src 'self'",
  "font-src 'self'",
  "img-src 'self' data:",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self'",
  "frame-ancestors 'none'",
].join('; ');

function sendJson(res, status, payload) {
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'x-content-type-options': 'nosniff',
  });
  res.end(JSON.stringify(payload));
}

function serveStatic(req, res, pathname) {
  const rel = pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, '');
  const file = path.join(PUBLIC_DIR, rel);
  // 目录穿越防护：解析后的路径必须还在 public 里面。
  if (!file.startsWith(PUBLIC_DIR + path.sep) && file !== path.join(PUBLIC_DIR, 'index.html')) {
    res.writeHead(403, { 'content-type': 'text/plain; charset=utf-8' });
    res.end('forbidden');
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
      'content-security-policy': CSP,
      'x-content-type-options': 'nosniff',
      'referrer-policy': 'no-referrer',
    });
    res.end(buf);
  });
}

/** 按 Buffer 收请求体，超限直接拒。字符串拼接会劈开多字节字符。 */
function readBody(req) {
  return new Promise((resolve) => {
    const chunks = [];
    let bytes = 0;
    let killed = false;
    req.on('data', (chunk) => {
      if (killed) return;
      bytes += chunk.length;
      if (bytes > LIMITS.requestBytes) {
        killed = true;
        // 不 destroy：连接断了 413 就发不出去。丢掉剩下的数据即可。
        req.resume();
        resolve({ tooLarge: true });
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      if (killed) return;
      const raw = Buffer.concat(chunks).toString('utf8');
      try {
        resolve({ body: raw ? JSON.parse(raw) : {} });
      } catch (_) {
        resolve({ body: {} });
      }
    });
    req.on('error', () => {
      if (!killed) resolve({ body: {} });
    });
  });
}

/**
 * 每个请求都包在这里面。
 *
 * `new URL()` 在请求行畸形时会抛（比如 `GET // HTTP/1.1`），而这个回调是 async——
 * 抛出去就是一个未处理的 Promise 拒绝，默认配置下整个进程会退出。
 * 一个畸形请求不该把整个工具带走。
 */
const server = http.createServer((req, res) => {
  handleRequest(req, res).catch((err) => {
    try {
      sendJson(res, 500, {
        ok: false,
        error: { code: 'BridgeError', message: String((err && err.message) || err) },
      });
    } catch (_) {
      // 响应已经发出去了，只能放弃这一条；进程要活着。
    }
  });
});

async function handleRequest(req, res) {
  let url;
  try {
    url = new URL(req.url, 'http://127.0.0.1');
  } catch (_) {
    return sendJson(res, 400, {
      ok: false,
      error: { code: 'BadRequestTarget', message: '无法解析的请求目标。' },
    });
  }

  if (url.pathname.startsWith('/api/')) {
    const denial = guardRequest(req);
    if (denial) return sendJson(res, 403, { ok: false, error: denial });

    const handler = routes[`${req.method} ${url.pathname}`];
    if (!handler) {
      return sendJson(res, 404, {
        ok: false,
        error: { code: 'NoRoute', message: `${req.method} ${url.pathname}` },
      });
    }

    // 演示模式下除了 /api/health 一律不执行命令，前端自己用内置样本数据。
    if (runtime.mode === 'demo' && url.pathname !== '/api/health') {
      return sendJson(res, 200, {
        ok: false,
        error: { code: 'DemoMode', message: runtime.reason || '当前是演示模式，没有连接真实项目。' },
      });
    }

    try {
      let body = null;
      if (req.method === 'POST') {
        const read = await readBody(req);
        if (read.tooLarge) {
          return sendJson(res, 413, {
            ok: false,
            error: { code: 'BodyTooLarge', message: `请求体超过 ${LIMITS.requestBytes} 字节。` },
          });
        }
        body = read.body;
      }
      return sendJson(res, 200, await handler(url, body));
    } catch (err) {
      return sendJson(res, 200, {
        ok: false,
        error: { code: 'BridgeError', message: String(err.message) },
      });
    }
  }

  serveStatic(req, res, url.pathname);
}

// ───────────────────────── 启动 ─────────────────────────

function start() {
  return detectAwr().then(
    () =>
      new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(ARGS.port, '127.0.0.1', () => resolve(server));
      })
  );
}

if (require.main === module) {
  start().then(
    () => {
      const addr = `http://127.0.0.1:${ARGS.port}`;
      console.log('');
      console.log('  AWR Inspector 已启动');
      console.log('  ─────────────────────────────────────────');
      console.log(`  地址    ${addr}`);
      console.log(`  项目    ${runtime.project}`);
      if (runtime.mode === 'live') {
        console.log(`  模式    真实数据（${runtime.awrVersion || 'awr'}）`);
      } else {
        console.log('  模式    演示模式');
        console.log(`  原因    ${runtime.reason}`);
      }
      console.log(`  重新索引 ${runtime.allowReindex ? '已开启' : '已关闭（--allow-reindex 开启）'}`);
      console.log('  ─────────────────────────────────────────');
      console.log('  按 Ctrl+C 停止');
      console.log('');
      if (ARGS.open) {
        const opener =
          process.platform === 'darwin' ? 'open'
            : process.platform === 'win32' ? 'explorer' : 'xdg-open';
        spawn(opener, [addr], { stdio: 'ignore', detached: true }).on('error', () => {});
      }
    },
    (err) => {
      if (err && err.code === 'EADDRINUSE') {
        console.error(`端口 ${ARGS.port} 已被占用。换一个：node server.js --port ${ARGS.port + 1}`);
      } else {
        console.error(err && err.message);
      }
      process.exit(1);
    }
  );
}

module.exports = { server, start, runtime, LIMITS, GUARD_HEADER, ARGS };
