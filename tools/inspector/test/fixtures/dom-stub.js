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
    this.value = '';
    this.own = ''; // 自身文本，不含子节点
  }

  set textContent(v) {
    this.own = v == null ? '' : String(v);
    this.children = [];
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
    return child;
  }

  removeChild(child) {
    this.children = this.children.filter((c) => c !== child);
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
    for (const fn of this.listeners.click || []) fn({ target: this });
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
  for (const id of IDS) byId.set(id, new StubElement('div'));

  const document = {
    getElementById: (id) => {
      if (!byId.has(id)) byId.set(id, new StubElement('div'));
      return byId.get(id);
    },
    createElement: (tag) => new StubElement(tag),
    querySelectorAll: () => [],
    querySelector: () => null,
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

  return { document, byId };
}

module.exports = { install, StubElement };
