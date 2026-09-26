/**
 * Regression coverage for measured packet size on the Context page.
 * The old Overview comparison depended on demo-only context_sample values.
 * Measurements now appear next to the compilation action that produces them.
 * Run: node --test test/packet-size.test.js
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

/** A compilation response matching the real API shape. */
function compileResponse({ total = 6256, required = 1930, budget = 16000, omitted = [] } = {}) {
  return {
    ok: true,
    command: 'awr context compile --work RECON-040',
    data: {
      ok: true,
      project_revision: 16,
      completeness: { complete: true, status: 'CONTEXT COMPLETE', issues: [], evidence_gaps: [] },
      work_context: {
        rendered_context: '# Packet content',
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

test('the initial empty state only promises supported measurements', () => {
  app.renderPacketSize(null);

  assert.equal($('ctxBig').textContent, '—');
  assert.ok(chartText().includes('Not compiled yet'));
  // Live projects do not provide corpus-size comparisons.
  assert.ok(!chartText().includes('corpus comparison'), `The empty state must not promise corpus comparisons: ${chartText()}`);
  assert.ok(chartText().includes('measured size'), 'Explain that compilation displays measured packet size');
});

test('compilation updates measurements without changing views', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });

  await app.doCompile();

  assert.equal($('ctxBig').textContent, '6,256');
  assert.ok($('ctxCap').textContent.includes('RECON-040'), 'Identify the work item beside the token count');
  assert.equal($('sizeSub').textContent, '39% of budget used');

  const text = chartText();
  assert.ok(text.includes('Required content') && text.includes('1,930 tokens'), text);
  assert.ok(text.includes('Included content') && text.includes('6,256 tokens'), text);
  assert.ok(text.includes('Budget limit') && text.includes('16,000 tokens'), text);
});

test('all three values come directly from AWR', async () => {
  global.fetch = async () => ({
    json: async () => compileResponse({ total: 4998, required: 1561, budget: 5000 }),
  });
  await app.doCompile();

  const text = chartText();
  assert.ok(text.includes('1,561 tokens'), 'Required tokens come from required_tokens');
  assert.ok(text.includes('4,998 tokens'), 'Included tokens come from token_estimate');
  assert.ok(text.includes('5,000 tokens'), 'Budget comes from token_budget');
  // Corpus size is unavailable and must not be invented.
  assert.ok(!text.includes('entire source corpus'), 'Do not show unsupported corpus comparisons for live projects');
});

test('the footnote reports the number of omitted chunks', async () => {
  const omitted = [
    { key: 'change:A:B', section: 'delta', reason: 'insufficient_budget_for_whole_chunk' },
    { key: 'change:C:D', section: 'delta', reason: 'insufficient_budget_for_whole_chunk' },
  ];
  global.fetch = async () => ({ json: async () => compileResponse({ omitted }) });
  await app.doCompile();

  assert.ok(noteText().includes('Omitted 2 chunks'), noteText());
});

test('the footnote explicitly reports no omissions', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });
  await app.doCompile();
  assert.ok(noteText().includes('No content was omitted'), noteText());
});

test('new compilation replaces prior measurements', async () => {
  global.fetch = async () => ({ json: async () => compileResponse() });
  await app.doCompile();
  assert.equal($('ctxBig').textContent, '6,256');

  global.fetch = async () => ({
    json: async () => compileResponse({ total: 900, required: 700, budget: 4000 }),
  });
  $('fBudget').value = '4000';
  await app.doCompile();
  assert.equal($('ctxBig').textContent, '900', 'Previous measurements must be cleared');
  assert.ok(chartText().includes('4,000 tokens'));
  assert.ok(!chartText().includes('16,000 tokens'), 'Previous budget must be cleared');
});

// Page consistency

const fs = require('node:fs');
const path = require('node:path');

test('every help button has an explanatory paragraph', () => {
  // Each data-why button must target a matching data-note paragraph.
  const html = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf8');
  const whys = [...html.matchAll(/data-why="([^"]+)"/g)].map((m) => m[1]);
  const notes = new Set([...html.matchAll(/data-note="([^"]+)"/g)].map((m) => m[1]));

  assert.ok(whys.length > 0, 'No help buttons found; check the selector');
  const orphans = whys.filter((w) => !notes.has(w));
  assert.deepEqual(orphans, [], `Help buttons without targets: ${orphans.join(', ')}`);
});

test('every explanatory paragraph is reachable', () => {
  const html = fs.readFileSync(path.join(__dirname, '..', 'public', 'index.html'), 'utf8');
  const whys = new Set([...html.matchAll(/data-why="([^"]+)"/g)].map((m) => m[1]));
  const notes = [...html.matchAll(/data-note="([^"]+)"/g)].map((m) => m[1]);

  const unreachable = notes.filter((n) => !whys.has(n));
  assert.deepEqual(unreachable, [], `Explanatory paragraphs without buttons: ${unreachable.join(', ')}`);
});

test('main content has no fixed maximum width', () => {
  // A fixed 1120px cap previously left excess whitespace on wide screens.
  const css = fs.readFileSync(path.join(__dirname, '..', 'public', 'styles.css'), 'utf8');
  const mainRule = css.match(/\nmain \{[^}]*\}/);
  assert.ok(mainRule, 'No main style rule found');
  assert.ok(!/max-width/.test(mainRule[0]), `main must not have a maximum width: ${mainRule[0].trim()}`);
});

// Budget and stale-result regressions

/** BudgetExceeded has details.required but no compilation report. */
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

test('retry budget respects the AWR limit when 95,000 tokens are required', async () => {
  const budgets = [];
  global.fetch = async (url, opts) => {
    const body = JSON.parse(opts.body);
    budgets.push(body.budget);
    // First request exceeds the budget; the button-triggered retry succeeds.
    return {
      json: async () => (budgets.length === 1
        ? budgetExceeded(95000, 16000)
        : compileResponse({ total: 96000, required: 95000, budget: 100000 })),
    };
  };

  await app.doCompile();

  const btn = $('breakdown').find(
    (el) => el.tagName === 'BUTTON' && el.textContent.includes('and recompile')
  );
  assert.ok(btn, 'Offer retry when 95,000 required tokens fit the limit');
  // Clamp 10% headroom (104,500) to the shared 100,000-token limit.
  assert.ok(btn.textContent.includes('100,000'), `Button text: ${btn.textContent}`);

  btn.click();
  await new Promise((r) => setTimeout(r, 0));
  // Allow doCompile to finish.
  await new Promise((r) => setTimeout(r, 0));

  assert.equal(budgets[1], 100000, `Actual retry budget: ${budgets[1]}`);
  assert.equal($('ctxBig').textContent, '96,000', 'The retry must succeed and display its result');
});

test('do not offer retry when required content exceeds the hard limit', async () => {
  global.fetch = async () => ({ json: async () => budgetExceeded(120000, 100000) });
  await app.doCompile();

  const box = $('breakdown');
  const btn = box.find((el) => el.tagName === 'BUTTON' && el.textContent.includes('and recompile'));
  assert.equal(btn, null, '120,000 exceeds the limit; no retry budget can succeed');
  assert.ok(box.textContent.includes('exceeding the AWR limit'), box.textContent);
  assert.ok(box.textContent.includes('120,000'), 'Display the required token count');
});

test('failed compilation clears previous measurements and omissions', async () => {
  // First compilation succeeds with one omitted chunk.
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

  // The next compilation fails without a report to render.
  $('fWork').value = 'RECON-013';
  global.fetch = async () => ({
    json: async () => ({
      ok: false,
      command: 'awr context compile --work RECON-013',
      error: { code: 'BudgetExceeded', message: 'context budget exceeded: required 120000, budget 4000', details: { required: 120000, budget: 4000 } },
    }),
  });
  await app.doCompile();

  // Clear every prior measurement, omission and preview.
  assert.equal($('ctxBig').textContent, '—', 'Token count still shows the previous 1,234');
  assert.ok(!chartText().includes('1,234'), `Size chart still contains prior measurements: ${chartText()}`);
  assert.equal($('omittedBox').hidden, true, 'Hide the omissions disclosure');
  assert.ok(!$('omittedList').textContent.includes('change:AAA:BBB'), 'Prior omission IDs remain');
  assert.equal($('packetTotal').textContent, '', 'Header token count remains');
  assert.equal($('packetPreview').textContent, '', 'Prior rendered text remains');
  // Keep the current error visible.
  assert.ok($('breakdown').textContent.includes('BudgetExceeded'));
});
