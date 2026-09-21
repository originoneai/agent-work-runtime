#!/usr/bin/env node
/**
 * 测试用的假 awr。
 *
 * 按 AWR 0.4.0 的参数签名回应，并且能按需制造几种边界情况：
 * 多字节输出、超大输出、慢命令、stderr 上的 JSON 错误。
 *
 * 环境变量：
 *   STUB_MODE=multibyte   一个字节一个字节地吐出合法 UTF-8 JSON
 *   STUB_MODE=huge        吐出超过桥接上限的输出
 *   STUB_MODE=slow        睡到超时之后才退出
 *   STUB_MODE=stderrjson  把 JSON 错误写到 stderr 并非零退出
 *   STUB_ARGV_OUT=<path>  把收到的 argv 原样写到这个文件，供断言用
 */

'use strict';

const fs = require('fs');

const argv = process.argv.slice(2);
const mode = process.env.STUB_MODE || '';

if (process.env.STUB_ARGV_OUT) {
  fs.appendFileSync(process.env.STUB_ARGV_OUT, JSON.stringify(argv) + '\n');
}

if (argv.includes('--version')) {
  process.stdout.write('awr 0.4.0-stub\n');
  process.exit(0);
}

const joined = argv.join(' ');

function emit(obj) {
  process.stdout.write(JSON.stringify(obj));
  process.exit(0);
}

if (mode === 'stderrjson') {
  process.stderr.write(JSON.stringify({ code: 'SourceStale', message: 'a source changed' }));
  process.exit(3);
}

if (mode === 'slowwrite') {
  // 活得比写超时更久。用来验证：槽位是按子进程释放的，不是按响应释放的。
  setTimeout(() => process.exit(0), 30 * 1000);
  setInterval(() => {}, 1000);
  return;
}

if (mode === 'hugewrite') {
  // 写命令的输出溢出。桥接不该杀它——先等一下再吐，好让超时分支也能覆盖到。
  const delay = Number(process.env.STUB_WRITE_DELAY_MS || 0);
  setTimeout(() => {
    const block = Buffer.alloc(1024 * 1024, 0x61);
    for (let i = 0; i < 12; i++) {
      try {
        fs.writeSync(1, block);
      } catch (e) {
        if (e.code === 'EPIPE') break;
        throw e;
      }
    }
    // 故意再多活一会儿，这样测试能观察到它没有被 SIGKILL。
    setTimeout(() => process.exit(0), 3000);
  }, delay);
  setInterval(() => {}, 1000);
  return;
}

if (mode === 'incomplete' && joined.includes('context compile')) {
  // 复现 `context compile` 的真实行为：上下文不完整时退出 1，
  // 但 stdout 上照样给出完整报告。
  fs.writeSync(1,
    JSON.stringify({
      ok: false,
      project_revision: 9,
      error: { code: 'ContextIncomplete', message: 'context incomplete: L1 has required gaps' },
      completeness: {
        complete: false,
        status: 'CONTEXT INCOMPLETE',
        project_revision: 9,
        rules_complete: false,
        acceptance_complete: true,
        issues: [{ code: 'hard_rule_unresolved', field: 'rules_complete' }],
        evidence_gaps: [],
        unresolved_required_dependencies: [],
      },
      work_context: {
        rendered_context: '# 不完整但仍然有内容',
        token_estimate: 120,
        required_tokens: 100,
        token_budget: 8000,
        selected_chunks: [{ key: 'rules/a', section: 'rules', required: true }],
        omitted_chunks: [],
      },
    })
  );
  process.exit(1);
}

if (mode === 'huge') {
  // 远超 stdout 上限。用 writeSync：process.stdout.write 是异步的，
  // 紧接着 process.exit() 会把还在管道缓冲里的数据丢掉。
  const block = Buffer.alloc(1024 * 1024, 0x61);
  for (let i = 0; i < 12; i++) {
    try {
      fs.writeSync(1, block);
    } catch (e) {
      if (e.code === 'EPIPE') break; // 桥接已经因为超限把我们杀了
      throw e;
    }
  }
  process.exit(0);
}

if (mode === 'slow') {
  setTimeout(() => process.exit(0), 60 * 1000);
  // 保持进程存活
  setInterval(() => {}, 1000);
} else if (mode === 'multibyte') {
  // 合法 UTF-8，逐字节写出。桥接如果按块解码就会得到 U+FFFD。
  const payload = Buffer.from(
    JSON.stringify({ ok: true, title: '任务：源文件索引 — αβγ 🧭', project_revision: 7 }),
    'utf8'
  );
  for (let i = 0; i < payload.length; i++) {
    fs.writeSync(1, payload.subarray(i, i + 1));
  }
  process.exit(0);
} else if (joined.includes('search')) {
  // AWR 0.4.0: `awr search [OPTIONS] [TEXT]` —— 文本是位置参数
  if (argv.includes('--text')) {
    process.stderr.write(
      JSON.stringify({ code: 'InvalidInput', message: "unexpected argument '--text' found" })
    );
    process.exit(2);
  }
  const dashDash = argv.indexOf('--');
  const text = dashDash >= 0 ? argv[dashDash + 1] : null;
  emit({ ok: true, hits: [], query: { text } });
} else if (joined.includes('status')) {
  emit({
    ok: true,
    project_revision: 7,
    current: [],
    current_total: 0,
    ready_count: 1,
    blocked_count: 0,
    organization: { state: 'ready', gaps: [], gap_total: 0, sources: [] },
  });
} else if (joined.includes('ready')) {
  emit({ ok: true, ready: [], ready_total: 0, blocked_sample: [], blocked_total: 0 });
} else if (joined.includes('work show')) {
  emit({ ok: true, project_revision: 7, work: { external_key: argv[argv.length - 1] }, acceptance: [] });
} else if (joined.includes('intake inspect')) {
  emit({ ok: true, project_revision: 7, organization: { sources: [], gaps: [], gap_total: 0 } });
} else if (joined.includes('context compile')) {
  emit({
    ok: true,
    project_revision: 7,
    completeness: { complete: true, project_revision: 7 },
    work_context: {
      rendered_context: 'stub',
      token_estimate: 10,
      token_budget: 5000,
      required_tokens: 10,
      selected_chunks: [],
      omitted_chunks: [],
    },
  });
} else if (joined.includes('source reindex')) {
  emit({ ok: true, project_revision: 8 });
} else {
  process.stderr.write('unknown stub command\n');
  process.exit(2);
}
