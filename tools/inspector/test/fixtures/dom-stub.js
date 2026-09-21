/**
 * 够用就好的 DOM 替身。
 *
 * 只为在 Node 里跑 app.js 的详情渲染路径——不追求实现 DOM 规范，
 * 只实现那段代码真正用到的那几个方法。
 *
 * 用法：在 require('../public/app.js') 之前调用 install()。
 */

'use strict';

class StubElement {
  constructor(tag) {
    this.tagName = String(tag || 'div').toUpperCase();
    this.className = '';
    this.children = [];
    this.style = {};
    this.dataset = {};
    this.attributes = {};
    this.listeners = {};
    this.hidden = false;
    this._value = '';
    this.own = ''; // 自身文本，不含子节点
  }

  get value() {
    return this._value;
  }

  set value(value) {
    value = String(value);
    this._value = this.tagName === 'SELECT' && !this.children.some(c => c.value === value)
      ? '' : value;
  }

  set textContent(v) {
    this.own = v == null ? '' : String(v);
    this.children = [];
    if (this.tagName === 'SELECT') this._value = '';
  }

  get textContent() {
    return this.own + this.children.map((c) => c.textContent).join('');
  }

  get innerText() {
    return this.textContent;
  }

  get firstChild() {
    return this.children.length ? this.children[0] : null;
  }

  appendChild(child) {
    this.children.push(child);
    if (this.tagName === 'SELECT' && this.children.length === 1) this._value = child.value;
    return child;
  }

  removeChild(child) {
    this.children = this.children.filter((c) => c !== child);
    if (this.tagName === 'SELECT' && child.value === this._value) this._value = this.children[0]?.value || '';
    return child;
  }

  setAttribute(name, value) {
    this.attributes[name] = String(value);
  }

  getAttribute(name) {
    return Object.prototype.hasOwnProperty.call(this.attributes, name) ? this.attributes[name] : null;
  }

  addEventListener(type, fn) {
    (this.listeners[type] = this.listeners[type] || []).push(fn);
  }

  click() {
    return Promise.all((this.listeners.click || []).map(fn => fn({ target: this })));
  }

  querySelectorAll() {
    return [];
  }

  querySelector() {
    return null;
  }

  /** 递归找第一个满足条件的后代，测试里用来定位按钮。 */
  find(predicate) {
    for (const child of this.children) {
      if (predicate(child)) return child;
      const hit = child.find(predicate);
      if (hit) return hit;
    }
    return null;
  }
}

/** app.js 会 getElementById 的那些。 */
const IDS = [
  'workDetail', 'detailId', 'detailStatus', 'rawWorkBody', 'rawWorkBody',
  'fWork', 'fGoal', 'fBudget', 'fIntent', 'cliMirror',
  'view-overview', 'view-work', 'view-context', 'view-sources',
  'workRows', 'workFilters', 'workSub', 'workEmpty',
];

function install() {
  const byId = new Map();
  for (const id of IDS) byId.set(id, new StubElement(id === 'fWork' ? 'select' : 'div'));

  // 按选择器取的元素（app.js 用 [data-note="..."] 找说明段落）。
  const bySelector = new Map();
  for (const note of ['contextChart', 'queues', 'checkpoints', 'mcp', 'criteria', 'completeness', 'pending']) {
    bySelector.set(`[data-note="${note}"]`, new StubElement('p'));
  }

  const document = {
    getElementById: (id) => {
      if (!byId.has(id)) byId.set(id, new StubElement('div'));
      return byId.get(id);
    },
    createElement: (tag) => new StubElement(tag),
    createTextNode: (text) => {
      const node = new StubElement('#text');
      node.textContent = text;
      return node;
    },
    querySelectorAll: () => [],
    querySelector: (sel) => bySelector.get(sel) || null,
    addEventListener: () => {},
    documentElement: new StubElement('html'),
  };

  global.document = document;
  global.window = {
    AWR_DEMO: undefined,
    scrollTo: () => {},
    matchMedia: () => ({ matches: false }),
    confirm: () => true,
    addEventListener: () => {},
  };
  global.history = { replaceState: () => {} };
  global.location = { hash: '' };
  global.localStorage = {
    getItem: () => null,
    setItem: () => {},
    removeItem: () => {},
  };
  global.sessionStorage = global.localStorage;
  // Node 自带只读的 navigator，直接赋值会抛。用 defineProperty 覆盖。
  Object.defineProperty(global, 'navigator', {
    value: { clipboard: null },
    configurable: true,
    writable: true,
  });

  return { document, byId, bySelector };
}

module.exports = { install, StubElement };
