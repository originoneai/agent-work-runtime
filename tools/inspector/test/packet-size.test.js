/**
 * 上下文页「这个包有多大」的回归。
 *
 * 这块度量原来在概览页，只读 `context_sample`——一个只存在于演示数据里的字段——
 * 所以真实项目上永远是空的，空态却写着「去编译一次这里就会显示」。
 * 现在它就画在编译按钮下面，和产生它的动作同一页。
 *
 * 跑：node --test test/packet-size.test.js
 */

'use strict';

const { test, beforeEach } = require('node:test');
const assert = require('node:assert');

const { install } = require('./fixtures/dom-stub.js');
install();

const app = require('../public/app.js');

const $ = (id) => document.getElementById(id);
const chartText = () => $('sizeChart').textContent;
const noteText = () => $('sizeNote').textContent;

/** 一份真实形状的编译响应。 */
function compileResponse({ total = 6256, required = 1930, budget = 16000, omitted = [] } = {}) {
  return {
    ok: true,
    command: 'awr context compile --work RECON-040',
    data: {
      ok: true,
      project_revision: 16,
      completeness: { complete: true, status: 'CONTEXT COMPLETE', issues: [], evidence_gaps: [] },
      work_context: {
        rendered_context: '# 包的正文',
        token_estimate: total,
        required_tokens: required,
        token_budget: budget,
        selected_chunks: [{ key: 'rules/a', section: 'rules', required: true }],
        omitted_chunks: omitted,
      },
    },
  };
}

beforeEach(() => {
  app.state.mode = 'live';
  app.state.compile = null;
  app.state.status = {};
  app.state.raw = {};
  $('sizeChart').textContent = '';
  $('fWork').textContent = '';
  for (const key of ['RECON-040', 'RECON-013']) {
    const option = document.createElement('option');
    option.value = key;
    $('fWork').appendChild(option);
  }
  $('fWork').value = 'RECON-040';
  $('fGoal').value = '';
  $('fBudget').value = '16000';
  $('fIntent').value = '';
});

test('还没编译时，空态不承诺做不到的事', () => {
  app.renderPacketSize(null);

  assert.equal($('ctxBig').textContent, '—');
  assert.ok(chartText().includes('还没有编译'));
  // 旧文案说「显示这个项目自己的体积对比」——那个对比在真实项目上造不出来，
  // 不能再出现在空态里。
  assert.ok(!chartText().includes('体积对比'), `空态不该再提「体积对比」：${chartText()}`);
  assert.ok(chartText().includes('实测体积'), '应说明编译后显示的是这个包的实测值');
});

test('编译之后同一页就填上了，不用切页面', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });

  await app.doCompile();

  assert.equal($('ctxBig').textContent, '6,256');
  assert.ok($('ctxCap').textContent.includes('RECON-040'), '大数字旁边要写明是哪个工作项');
  assert.equal($('sizeSub').textContent, '用掉预算的 39%');

  const text = chartText();
  assert.ok(text.includes('必需内容') && text.includes('1,930 tokens'), text);
  assert.ok(text.includes('这次装进去的') && text.includes('6,256 tokens'), text);
  assert.ok(text.includes('预算上限') && text.includes('16,000 tokens'), text);
});

test('三条都来自 AWR，不做推算', async () => {
  global.fetch = async () => ({
    json: async () => compileResponse({ total: 4998, required: 1561, budget: 5000 }),
  });
  await app.doCompile();

  const text = chartText();
  assert.ok(text.includes('1,561 tokens'), 'required 要原样取自 required_tokens');
  assert.ok(text.includes('4,998 tokens'), 'total 要原样取自 token_estimate');
  assert.ok(text.includes('5,000 tokens'), 'budget 要原样取自 token_budget');
  // 语料体积测不出来，不许出现这类条目
  assert.ok(!text.includes('读完整源码'), '真实项目不得出现无法测量的对比条');
});

test('有内容被省略时，注脚说出省了几块', async () => {
  const omitted = [
    { key: 'change:A:B', section: 'delta', reason: 'insufficient_budget_for_whole_chunk' },
    { key: 'change:C:D', section: 'delta', reason: 'insufficient_budget_for_whole_chunk' },
  ];
  global.fetch = async () => ({ json: async () => compileResponse({ omitted }) });
  await app.doCompile();

  assert.ok(noteText().includes('省略了 2 块'), noteText());
});

test('没有省略时也说清楚', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });
  await app.doCompile();
  assert.ok(noteText().includes('没有内容被省略'), noteText());
});

test('换一次编译，数字跟着换', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });
  await app.doCompile();
  assert.equal($('ctxBig').textContent, '6,256');

  global.fetch = async () => ({
    json: async () => compileResponse({ total: 900, required: 700, budget: 4000 }),
  });
  $('fBudget').value = '4000';
  await app.doCompile();
  assert.equal($('ctxBig').textContent, '900', '旧的数不该留在页面上');
  assert.ok(chartText().includes('4,000 tokens'));
  assert.ok(!chartText().includes('16,000 tokens'), '上一次的预算不该还在');
});

// ───────────── 页面自身的一致性 ─────────────

const fs = require('node:fs');
const path = require('node:path');

test('每个 ? 按钮都有对应的说明段落', () => {
  // 一个 data-why 找不到同名的 data-note，点了就什么也不会发生——
  // 这种「按钮是死的」只能靠这条断言发现，界面上看不出来。
  const html = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf8');
  const whys = [...html.matchAll(/data-why="([^"]+)"/g)].map((m) => m[1]);
  const notes = new Set([...html.matchAll(/data-note="([^"]+)"/g)].map((m) => m[1]));

  assert.ok(whys.length > 0, '没找到任何 ? 按钮，选择器是不是改了？');
  const orphans = whys.filter((w) => !notes.has(w));
  assert.deepEqual(orphans, [], `这些 ? 按钮点了没反应: ${orphans.join(', ')}`);
});

test('没有说明段落是孤立的', () => {
  const html = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf8');
  const whys = new Set([...html.matchAll(/data-why="([^"]+)"/g)].map((m) => m[1]));
  const notes = [...html.matchAll(/data-note="([^"]+)"/g)].map((m) => m[1]);

  const unreachable = notes.filter((n) => !whys.has(n));
  assert.deepEqual(unreachable, [], `这些说明没有按钮能打开: ${unreachable.join(', ')}`);
});

test('主区不设固定宽度上限', () => {
  // 之前 main 被钉在 1120px，宽屏上右边一大片空白。
  const css = fs.readFileSync(path.join(__dirname, '..', 'public', 'styles.css'), 'utf8');
  const mainRule = css.match(/\nmain \{[^}]*\}/);
  assert.ok(mainRule, '没找到 main 的样式规则');
  assert.ok(!/max-width/.test(mainRule[0]), `main 不该再有宽度上限: ${mainRule[0].trim()}`);
});

// ───────────── 复核提出的两个 P2 ─────────────

/** BudgetExceeded 的真实形状：没有报告，只有 details.required。 */
function budgetExceeded(required, budget) {
  return {
    ok: false,
    command: `awr context compile --work RECON-040 --budget ${budget}`,
    error: {
      code: 'BudgetExceeded',
      message: `context budget exceeded: required ${required}, budget ${budget}`,
      details: { required, budget },
    },
  };
}

test('重试按钮的预算不会超过 AWR 的上限（required = 95,000）', async () => {
  const budgets = [];
  global.fetch = async (url, opts) => {
    const body = JSON.parse(opts.body);
    budgets.push(body.budget);
    // 第一次：超预算；第二次（按钮触发）：成功
    return {
      json: async () => (budgets.length === 1
        ? budgetExceeded(95000, 16000)
        : compileResponse({ total: 96000, required: 95000, budget: 100000 })),
    };
  };

  await app.doCompile();

  const btn = $('breakdown').find(
    (el) => el.tagName === 'BUTTON' && el.textContent.includes('并重编译')
  );
  assert.ok(btn, '95,000 在上限之内，应该给重试按钮');
  // 1.1 倍余量算出来是 104,500，但桥接和 AWR 都是 100,000 封顶——按钮不能超
  assert.ok(btn.textContent.includes('100,000'), `按钮文案: ${btn.textContent}`);

  btn.click();
  await new Promise((r) => setTimeout(r, 0));
  // 让 doCompile 跑完
  await new Promise((r) => setTimeout(r, 0));

  assert.equal(budgets[1], 100000, `重试时实际发出的预算: ${budgets[1]}`);
  assert.equal($('ctxBig').textContent, '96,000', '重试之后应当成功并显示新结果');
});

test('必需内容超过上限时，不给一个必然失败的按钮', async () => {
  global.fetch = async () => ({ json: async () => budgetExceeded(120000, 100000) });
  await app.doCompile();

  const box = $('breakdown');
  const btn = box.find((el) => el.tagName === 'BUTTON' && el.textContent.includes('并重编译'));
  assert.equal(btn, null, '120,000 超过上限，任何预算都编不出来，不该有重试按钮');
  assert.ok(box.textContent.includes('超过 AWR 的上限'), box.textContent);
  assert.ok(box.textContent.includes('120,000'), '要把必需量说出来');
});

test('先成功再失败，上一次的体积和省略不能留在页面上', async () => {
  // 第一次：WORK-A 成功，带一块被省略的内容
  global.fetch = async () => ({
    json: async () => compileResponse({
      total: 1234,
      required: 900,
      budget: 4000,
      omitted: [{ key: 'change:AAA:BBB', section: 'delta', reason: 'insufficient_budget_for_whole_chunk' }],
    }),
  });
  await app.doCompile();
  assert.equal($('ctxBig').textContent, '1,234');
  assert.equal($('omittedBox').hidden, false);
  assert.ok($('omittedList').textContent.includes('change:AAA:BBB'));

  // 第二次：WORK-B 失败，且没有报告可渲染
  $('fWork').value = 'RECON-013';
  global.fetch = async () => ({
    json: async () => ({
      ok: false,
      command: 'awr context compile --work RECON-013',
      error: { code: 'BudgetExceeded', message: 'context budget exceeded: required 120000, budget 4000', details: { required: 120000, budget: 4000 } },
    }),
  });
  await app.doCompile();

  // 大数字、体积图、省略折叠区、头部计数、预览——全部不能是 A 的
  assert.equal($('ctxBig').textContent, '—', '大数字还停在上一次的 1,234');
  assert.ok(!chartText().includes('1,234'), `体积图还留着上一次的数: ${chartText()}`);
  assert.equal($('omittedBox').hidden, true, '省略折叠区应当隐藏');
  assert.ok(!$('omittedList').textContent.includes('change:AAA:BBB'), '上一次的省略 id 还在');
  assert.equal($('packetTotal').textContent, '', '头部的 token 计数还留着');
  assert.equal($('packetPreview').textContent, '', '上一次的渲染文本还留着');
  // 错误本身要显示出来
  assert.ok($('breakdown').textContent.includes('BudgetExceeded'));
});
