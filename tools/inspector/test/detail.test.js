/**
 * 详情面板的前端回归。
 *
 * 在 Node 里用一个最小 DOM 替身跑 app.js 真正的 renderWorkDetail()，
 * 而不是测一个抄过来的副本——抄的版本会跟着源码漂走。
 *
 * 跑：node --test test/detail.test.js
 */

'use strict';

const { test, beforeEach } = require('node:test');
const assert = require('node:assert');

const { install } = require('./fixtures/dom-stub.js');
install();

const app = require('../public/app.js');

/** 装一个假的 fetch，记录每次请求，按 key 返回对应的响应。 */
function stubFetch() {
  const calls = [];
  global.fetch = async (url) => {
    const key = decodeURIComponent(String(url).split('key=')[1] || '');
    calls.push(key);
    return {
      json: async () => ({
        ok: true,
        command: `awr work show ${key}`,
        data: {
          ok: true,
          project_revision: 7,
          // 打个标记，方便断言「这份原始响应属于谁」
          marker: `envelope-of-${key}`,
          work: {
            external_key: key,
            title: `标题 ${key}`,
            status: 'ready',
            next_action: `下一步 ${key}`,
            milestone: `goal#${key}`,
            active_claims: [],
            diagnostics: [],
          },
          acceptance: [`验收 ${key}`],
          required_dependencies: [],
          missing_dependencies: [],
          evidence: [],
          decisions: [],
          dependency_cycles: [],
        },
      }),
    };
  };
  return calls;
}

beforeEach(() => {
  app.state.mode = 'live';
  app.state.workDetail = {};
  app.state.raw = {};
  app.state.selectedWork = null;
  app.detailGuard.invalidate();
});

test('缓存命中时原始响应跟着一起恢复', async () => {
  const calls = stubFetch();
  const rawPanel = document.getElementById('rawWorkBody');
  const detailBox = document.getElementById('workDetail');

  // 1) 选 A，等它回来
  await app.renderWorkDetail('EXAMPLE-A');
  assert.equal(app.state.raw.work.data.marker, 'envelope-of-EXAMPLE-A');

  // 2) 选 B，等它回来
  await app.renderWorkDetail('EXAMPLE-B');
  assert.equal(app.state.raw.work.data.marker, 'envelope-of-EXAMPLE-B');

  // 3) 再选 A —— 这次走缓存
  await app.renderWorkDetail('EXAMPLE-A');

  assert.equal(calls.length, 2, `第三次选择不应再发请求，实际请求: ${JSON.stringify(calls)}`);

  // 标题、正文、原始响应、原始 JSON 面板必须全部指向 A
  assert.equal(document.getElementById('detailId').textContent, 'EXAMPLE-A');
  assert.ok(detailBox.textContent.includes('标题 EXAMPLE-A'), '正文应是 A');
  assert.ok(detailBox.textContent.includes('验收 EXAMPLE-A'), '验收标准应是 A');
  assert.ok(!detailBox.textContent.includes('EXAMPLE-B'), '正文里不该出现 B');

  assert.equal(
    app.state.raw.work.data.marker,
    'envelope-of-EXAMPLE-A',
    'state.raw.work 仍停在 B 的响应上'
  );
  assert.ok(
    rawPanel.textContent.includes('envelope-of-EXAMPLE-A'),
    '原始 JSON 面板仍显示 B 的响应'
  );
  assert.ok(
    !rawPanel.textContent.includes('envelope-of-EXAMPLE-B'),
    '原始 JSON 面板里不该还有 B 的响应'
  );
});

test('缓存命中时「为这一项编译上下文」指向的还是它自己', async () => {
  stubFetch();
  await app.renderWorkDetail('EXAMPLE-A');
  await app.renderWorkDetail('EXAMPLE-B');
  await app.renderWorkDetail('EXAMPLE-A');

  const box = document.getElementById('workDetail');
  // 按标签匹配：只看 textContent 会先命中包着按钮的那个容器。
  const btn = box.find((el) => el.tagName === 'BUTTON' && el.textContent.includes('为这一项编译上下文'));
  assert.ok(btn, '没找到编译按钮');
  btn.click();
  assert.equal(document.getElementById('fWork').value, 'EXAMPLE-A');
});

test('迟到的响应不会覆盖当前选中项（端到端，不只是守卫）', async () => {
  const calls = [];
  const resolvers = {};
  global.fetch = (url) => {
    const key = decodeURIComponent(String(url).split('key=')[1] || '');
    calls.push(key);
    return new Promise((resolve) => {
      resolvers[key] = () =>
        resolve({
          json: async () => ({
            ok: true,
            data: {
              marker: `envelope-of-${key}`,
              work: { external_key: key, title: `标题 ${key}`, status: 'ready', active_claims: [], diagnostics: [] },
              acceptance: [`验收 ${key}`],
              required_dependencies: [],
              missing_dependencies: [],
              evidence: [],
              decisions: [],
              dependency_cycles: [],
            },
          }),
        });
    });
  };

  const a = app.renderWorkDetail('EXAMPLE-A');
  const b = app.renderWorkDetail('EXAMPLE-B');

  // B 先回，A 后回
  resolvers['EXAMPLE-B']();
  await b;
  resolvers['EXAMPLE-A']();
  await a;

  const box = document.getElementById('workDetail');
  assert.ok(box.textContent.includes('标题 EXAMPLE-B'), '正文应停在 B');
  assert.ok(!box.textContent.includes('标题 EXAMPLE-A'), 'A 的迟到响应不该落地');
  assert.equal(app.state.raw.work.data.marker, 'envelope-of-EXAMPLE-B');
});

test('迟到的失败响应也不会盖掉当前选中项', async () => {
  const resolvers = {};
  global.fetch = (url) => {
    const key = decodeURIComponent(String(url).split('key=')[1] || '');
    return new Promise((resolve) => {
      resolvers[key] = () =>
        resolve({
          json: async () =>
            key === 'EXAMPLE-A'
              ? { ok: false, command: 'awr work show A', error: { code: 'SourceStale', message: '过期了' } }
              : {
                  ok: true,
                  data: {
                    marker: `envelope-of-${key}`,
                    work: { external_key: key, title: `标题 ${key}`, status: 'ready', active_claims: [], diagnostics: [] },
                    acceptance: [],
                    required_dependencies: [],
                    missing_dependencies: [],
                    evidence: [],
                    decisions: [],
                    dependency_cycles: [],
                  },
                },
        });
    });
  };

  const a = app.renderWorkDetail('EXAMPLE-A');
  const b = app.renderWorkDetail('EXAMPLE-B');
  resolvers['EXAMPLE-B']();
  await b;
  resolvers['EXAMPLE-A']();
  await a;

  const box = document.getElementById('workDetail');
  assert.ok(box.textContent.includes('标题 EXAMPLE-B'), '正文应停在 B');
  assert.ok(!box.textContent.includes('SourceStale'), '迟到的失败不该显示出来');
  assert.equal(app.state.raw.work.data.marker, 'envelope-of-EXAMPLE-B');
});

test('刷新后，刷新前发出的同 key 请求不算数', async () => {
  const resolvers = {};
  global.fetch = (url) => {
    const key = decodeURIComponent(String(url).split('key=')[1] || '');
    return new Promise((resolve) => {
      resolvers[key] = (marker) =>
        resolve({
          json: async () => ({
            ok: true,
            data: {
              marker,
              work: { external_key: key, title: marker, status: 'ready', active_claims: [], diagnostics: [] },
              acceptance: [],
              required_dependencies: [],
              missing_dependencies: [],
              evidence: [],
              decisions: [],
              dependency_cycles: [],
            },
          }),
        });
    });
  };

  const stale = app.renderWorkDetail('EXAMPLE-A');
  // 刷新：作废在途请求并清空缓存，和界面上的刷新按钮一样
  app.detailGuard.invalidate();
  app.state.workDetail = {};

  resolvers['EXAMPLE-A']('旧的-A');
  await stale;

  assert.equal(app.state.raw.work, undefined, '刷新前的响应不该落地');
  assert.equal(app.state.workDetail['EXAMPLE-A'], undefined, '刷新前的响应不该进缓存');
});
