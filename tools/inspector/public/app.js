/* ══════════════════════════════════════════════════════════
   AWR Console —— 前端逻辑
   ══════════════════════════════════════════════════════════ */

(function () {
  'use strict';

  // ───────────────────────────────────────────────────────
  // 字段映射
  //
  // status / work show / context compile 三条命令的路径是照着仓库源码核对过的：
  //   crates/awr-runtime/src/status_action.rs   （status --view action）
  //   crates/awr-runtime/src/status_summary.rs  （队列条目的 brief 形状）
  //   crates/awr-cli/src/query.rs               （work show）
  //   crates/awr-context/src/{compile,budget}.rs（context compile）
  // intake inspect 的形状没核完，Sources 视图的映射仍是推测的。
  //
  // 每个字段给了多个候选路径，取第一个能取到的。某一格显示「—」时：
  // 打开该视图底部的「原始 JSON」，看真实字段叫什么，加进对应数组即可。
  // 全部映射集中在这里，别处不猜字段。
  // ───────────────────────────────────────────────────────

  const FIELD_MAP = {
    status: {
      revision:     ['project_revision', 'revision'],
      currentTotal: ['current_total'],
      readyCount:   ['ready_count', 'counts.ready'],
      waitingCount: ['waiting_count'],
      blockedCount: ['blocked_count', 'counts.blocked'],
      selected:     ['total'],
      projectTotal: ['project_work_total'],
      gaps:         ['organization.gaps'],
      pendingOps:   ['pending_operations.items'],
      pendingTotal: ['pending_operations.total'],
      gapTotal:     ['organization.gap_total', 'organization.project_gap_total'],
      orgState:     ['organization.state'],
      freshness:    ['freshness_basis'],
      guidance:     ['guidance.next_action', 'next_action'],
    },
    // 两种 status 形状都要认：
    //  - 源码树（未发布）：四个顶层数组 current/ready/waiting/blocked + omissions
    //  - 发布版 0.4.0：只有 current；ready 和 blocked 要另外跑 `awr ready` 拿
    queueItems:     { current: ['current'], ready: ['ready'], waiting: ['waiting'], blocked: ['blocked'] },
    queueOmitted:   { current: ['omissions.current'], ready: ['omissions.ready'], waiting: ['omissions.waiting'], blocked: ['omissions.blocked'] },
    // `awr ready` 的响应
    readyCmd: {
      items:        ['ready'],
      total:        ['ready_total'],
      blockedItems: ['blocked_sample'],
      // 注意：blocked_total 是「不可选」（含已被 claim 的），与 status 的 blocked_count 定义不同，
      // 只用来给列表标注截断，不往状态条上放。
      blockedTotal: ['blocked_total'],
    },
    workItem: {
      key:        ['key', 'external_key', 'work', 'id'],
      title:      ['title', 'summary', 'name'],
      status:     ['status', 'state'],
      rawStatus:  ['raw_status'],
      owner:      ['owner'],
      queue:      ['queue', 'bucket'],
      revision:   ['revision', 'source_revision'],
      nextAction: ['next_action', 'next'],
      blocker:    ['blocker', 'block_reason'],
      waitTotal:  ['wait_total'],
      codes:      ['codes', 'structural_codes'],
      // work show 才有的
      goal:       ['milestone', 'goal', 'goal_summary'],
      acceptance: ['acceptance', 'acceptance_criteria', 'criteria'],
      dependsOn:  ['required_dependencies', 'depends_on', 'dependencies'],
      missingDeps:['missing_dependencies'],
      claims:     ['active_claims'],
      diagnostics:['diagnostics'],
      evidence:   ['evidence'],
      decisions:  ['decisions'],
      cycles:     ['dependency_cycles'],
    },
    context: {
      rendered:   ['work_context.rendered_context', 'rendered_context', 'diagnostic_text'],
      chunks:     ['work_context.selected_chunks', 'selected_chunks'],
      total:      ['work_context.token_estimate', 'token_estimate'],
      required:   ['work_context.required_tokens', 'required_tokens'],
      budget:     ['work_context.token_budget', 'token_budget'],
      hash:       ['work_context.context_hash', 'context_hash'],
      tokenizer:  ['work_context.tokenizer', 'tokenizer'],
      complete:   ['completeness.complete', 'ok'],
      statusText: ['completeness.status'],
      revision:   ['completeness.project_revision', 'project_revision'],
      omissions:  ['work_context.omitted_chunks', 'omitted_refs', 'omitted_chunks'],
      // 完整性是分维度给的，不是一个布尔。缺哪一维直接决定 agent 会不会瞎干。
      dimensions: ['completeness'],
      evidenceGaps: ['completeness.evidence_gaps'],
      unresolvedDeps: ['completeness.unresolved_required_dependencies'],
      issues:     ['completeness.issues'],
    },
    // `awr intake inspect` 返回的是组织报告，不是文件表。
    // 真正的源清单在 organization.sources[]：{domain, freshness, locator, revision, role}
    sources: {
      files:      ['organization.sources', 'files', 'sources', 'matched'],
      path:       ['locator', 'path', 'file'],
      kind:       ['domain', 'kind', 'type'],
      state:      ['freshness', 'state', 'status'],
      role:       ['role'],
      revision:   ['revision', 'source_revision'],
      rejection:  ['rejection', 'violation', 'error'],
      issues:     ['source_issues'],
      gapTotal:   ['organization.gap_total'],
    },
  };

  /** 按点分路径取值，`a.b.0.c` 这种也行。 */
  function at(obj, path) {
    if (obj == null) return undefined;
    return path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
  }

  /** 按候选路径数组取第一个非空值。 */
  function pick(obj, candidates, fallback) {
    for (const p of candidates || []) {
      const v = at(obj, p);
      if (v !== undefined && v !== null && v !== '') return v;
    }
    return fallback;
  }

  // ───────────────────────── 小工具 ─────────────────────────

  const $ = (id) => document.getElementById(id);
  const el = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  };
  const group = (n) => (n == null || isNaN(n) ? '—' : Math.round(n).toLocaleString('en-US'));

  /** 把时间戳变成「6 分钟前」这种。拿不到就原样回显。 */
  function since(value) {
    if (!value) return '—';
    const t = typeof value === 'number' ? value : Date.parse(value);
    if (isNaN(t)) return String(value);
    const min = Math.max(0, Math.round((Date.now() - t) / 60000));
    if (min < 1) return '刚刚';
    if (min < 60) return `${min} 分钟前`;
    const h = Math.round(min / 60);
    if (h < 24) return `${h} 小时前`;
    const d = Math.round(h / 24);
    return `${d} 天前`;
  }

  /** 面向未来的时间：claim 到期这种。过去了就说「已过期」。 */
  function until(value) {
    if (!value) return '';
    const t = typeof value === 'number' ? value : Date.parse(value);
    if (isNaN(t)) return String(value);
    const min = Math.round((t - Date.now()) / 60000);
    if (min <= 0) return '已过期';
    if (min < 60) return `${min} 分钟后到期`;
    const h = Math.round(min / 60);
    if (h < 24) return `${h} 小时后到期`;
    return `${Math.round(h / 24)} 天后到期`;
  }

  function setText(id, text) {
    const n = $(id);
    if (n) n.textContent = text;
  }

  function clear(node) {
    while (node && node.firstChild) node.removeChild(node.firstChild);
  }

  // ───────────────────────── 状态 ─────────────────────────

  const state = {
    mode: 'demo',          // 'live' | 'demo'
    project: '…',
    reason: null,
    status: null,          // 规范化后的 status
    works: [],             // 规范化后的工作项清单
    workDetail: {},        // key -> 详情
    sources: null,
    compile: null,
    raw: {},               // 每个视图最近一次的原始 JSON
    queueTab: 'blocked',   // 概览里队列面板当前选的队列
    workFilter: 'all',
    selectedWork: null,
    view: 'overview',
  };

  const QUEUES = [
    { key: 'current', label: '进行中', dot: 'info', why: '有人正在做' },
    { key: 'ready',   label: '可开工', dot: 'ok',   why: '依赖都满足了' },
    { key: 'waiting', label: '等待中', dot: 'warn', why: '在等一个回答或前置结果' },
    { key: 'blocked', label: '被阻塞', dot: 'crit', why: '真的卡住了' },
  ];
  const queueMeta = (k) => QUEUES.find((q) => q.key === k) || { label: k || '—', dot: '', why: '' };

  // ───────────────────────── 代际守卫 ─────────────────────────

  /**
   * 详情请求的代际守卫。
   *
   * 点了 A 再点 B，两个请求并发；如果 B 先回、A 后回，A 的响应会把 B 的面板覆盖掉，
   * 于是标题显示 B、正文却是 A——更糟的是「为这一项编译上下文」会按 A 走。
   *
   * 每次发起给一个递增的 token，回来时只认最新的那个，并且当前选中项必须还是它。
   * 刷新时调 invalidate()，让在途的旧请求全部作废。
   */
  function createGenerationGuard() {
    let generation = 0;
    let currentKey = null;
    return {
      begin(key) {
        generation += 1;
        currentKey = key;
        return { generation, key };
      },
      isCurrent(token) {
        return Boolean(token) && token.generation === generation && token.key === currentKey;
      },
      invalidate() {
        generation += 1;
        currentKey = null;
      },
    };
  }

  const detailGuard = createGenerationGuard();

  // ───────────────────────── API ─────────────────────────

  // 桥接要求状态变更请求带这个头。第三方页面发不出自定义头（会触发 CORS 预检，
  // 而桥接不给预检放行），所以它同时也是 CSRF 防护。
  const GUARD_HEADER = 'X-AWR-Inspector';

  async function callApi(path, options) {
    const opts = Object.assign({}, options);
    opts.headers = Object.assign(
      { 'content-type': 'application/json', [GUARD_HEADER]: '1' },
      opts.headers
    );
    try {
      const res = await fetch(path, opts);
      return await res.json();
    } catch (err) {
      return { ok: false, error: { code: 'BridgeUnreachable', message: String(err.message) } };
    }
  }

  // ───────────────────────── 规范化 ─────────────────────────

  function normWorkBrief(raw) {
    const M = FIELD_MAP.workItem;
    const waitTotal = pick(raw, M.waitTotal, 0);
    return {
      key: pick(raw, M.key, '—'),
      title: pick(raw, M.title, '（源文件里没有标题）'),
      status: pick(raw, M.status, null),
      rawStatus: pick(raw, M.rawStatus, null),
      owner: pick(raw, M.owner, null),
      queue: pick(raw, M.queue, null),
      revision: pick(raw, M.revision, null),
      nextAction: pick(raw, M.nextAction, null),
      blocker: pick(raw, M.blocker, null),
      waitTotal: waitTotal,
      // 诊断码是 AWR 说明「为什么卡住」的方式，比如 dependency_not_completed。
      codes: pick(raw, M.codes, []) || [],
      raw,
    };
  }

  function normWorkDetail(raw) {
    const M = FIELD_MAP.workItem;
    // work show 把条目本身放在 `work` 下，验收和依赖在同级。
    const item = raw && raw.work ? raw.work : raw;
    const base = normWorkBrief(item);

    // acceptance 在 AWR 里是 Vec<String>：只有标准文本，没有「已达成」状态。
    // 达成与否是在 complete 的时候逐条对证据验的，这里不假装知道。
    const acceptance = (pick(raw, M.acceptance, []) || []).map((a) =>
      typeof a === 'string'
        ? { criterion: a, evidence: [] }
        : { criterion: a.criterion || a.text || String(a), evidence: a.evidence || [] }
    );

    const deps = (pick(raw, M.dependsOn, []) || []).map((d) =>
      typeof d === 'string' ? { key: d } : { key: d.external_key || d.key, status: d.status }
    );

    return Object.assign(base, {
      // milestone/goal 在条目里，不在响应外层。
      goal: pick(item, M.goal, null) || pick(raw, M.goal, null),
      acceptance,
      dependsOn: deps,
      missingDeps: pick(raw, M.missingDeps, []) || [],
      cycles: pick(raw, M.cycles, []) || [],
      // 谁正在占着这件活。claim 是 AWR 的所有权凭证，没有它 agent 不能动手。
      claims: (pick(item, M.claims, []) || []).map((c) => ({
        id: c.id,
        session: c.session_id || c.session,
        agent: c.agent_id || c.agent,
        expiresAt: c.expires_at || null,
      })),
      diagnostics: pick(item, M.diagnostics, []) || [],
      evidence: pick(raw, M.evidence, []) || [],
      decisions: pick(raw, M.decisions, []) || [],
    });
  }

  /**
   * @param raw       `awr status` 的响应
   * @param readyRaw  `awr ready` 的响应；发布版 0.4.0 的 status 不带 ready/blocked 列表，
   *                  得靠它补。源码树版本自带四个数组，这时它只是冗余。
   */
  function normStatus(raw, readyRaw) {
    const M = FIELD_MAP.status;
    const R = FIELD_MAP.readyCmd;
    const queues = {};
    let all = [];

    // 队列条目：优先用 status 自己的数组；没有就退回 `awr ready`。
    const fallback = {
      ready: pick(readyRaw, R.items, null),
      blocked: pick(readyRaw, R.blockedItems, null),
    };

    for (const q of QUEUES) {
      let items = pick(raw, FIELD_MAP.queueItems[q.key], null);
      let omitted = pick(raw, FIELD_MAP.queueOmitted[q.key], 0) || 0;
      let source = 'status';

      if (items == null && fallback[q.key] != null) {
        items = fallback[q.key];
        source = 'ready';
        if (q.key === 'ready') {
          const t = pick(readyRaw, R.total, items.length);
          omitted = Math.max(0, t - items.length);
        }
      }
      // waiting 在发布版 0.4.0 里根本不存在，别拿 0 冒充「没有」。
      const available = items != null;
      items = items || [];

      const list = items.map(normWorkBrief).map((w) => Object.assign(w, { queue: q.key }));
      queues[q.key] = { total: list.length + omitted, omitted, items: list, available, source };
      all = all.concat(list);
    }

    // 计数以 AWR 自己给的为准，不用列表长度推算。缺就是缺，记成 null。
    const counted = {
      current: pick(raw, M.currentTotal, queues.current.available ? queues.current.total : null),
      ready: pick(raw, M.readyCount, queues.ready.available ? queues.ready.total : null),
      waiting: pick(raw, M.waitingCount, queues.waiting.available ? queues.waiting.total : null),
      blocked: pick(raw, M.blockedCount, queues.blocked.available ? queues.blocked.total : null),
    };
    for (const k of Object.keys(counted)) {
      if (counted[k] != null) queues[k].total = counted[k];
      else queues[k].available = false;
    }

    // 同一个 key 可能出现在多个队列里，去重，先到的赢。
    const seen = new Set();
    const works = all.filter((w) => (seen.has(w.key) ? false : (seen.add(w.key), true)));

    return {
      revision: pick(raw, M.revision, null),
      currentTotal: counted.current,
      readyCount: counted.ready,
      waitingCount: counted.waiting,
      blockedCount: counted.blocked,
      selected: pick(raw, M.selected, null),
      projectTotal: pick(raw, M.projectTotal, null),
      queues,
      works,
      gaps: pick(raw, M.gaps, []) || [],
      gapTotal: pick(raw, M.gapTotal, null),
      // 被中断、结果未知的运行时操作。有这个就得先去查，别急着重跑。
      pendingOps: pick(raw, M.pendingOps, []) || [],
      pendingTotal: pick(raw, M.pendingTotal, null),
      orgState: pick(raw, M.orgState, null),
      freshness: pick(raw, M.freshness, null),
      guidance: pick(raw, M.guidance, null),
      contextSample: raw && raw.context_sample ? raw.context_sample : null,
    };
  }

  function normSources(raw) {
    const M = FIELD_MAP.sources;
    const files = (pick(raw, M.files, []) || []).map((f) => {
      const locator = pick(f, M.path, '—');
      return {
        // locator 是 file:// URL，界面上显示成项目内的相对路径更好读。
        path: shortenLocator(locator),
        locator,
        kind: pick(f, M.kind, '—'),
        role: pick(f, M.role, null),
        revision: pick(f, M.revision, null),
        // AWR 的源状态叫 freshness：fresh / stale …
        state: String(pick(f, M.state, 'fresh')).toLowerCase(),
        rejection: pick(f, M.rejection, null),
      };
    });
    // 按 domain 汇总，代替原来按扩展名统计
    const summary = {};
    for (const f of files) summary[f.kind] = (summary[f.kind] || 0) + 1;

    return {
      files,
      summary: files.length ? summary : null,
      issues: pick(raw, M.issues, []) || [],
      gapTotal: pick(raw, M.gapTotal, null),
    };
  }

  /** file:///a/b/demo/RULES.md → demo/RULES.md；拿不到就原样。 */
  function shortenLocator(locator) {
    const s = String(locator || '');
    if (!s.startsWith('file://')) return s;
    const parts = s.replace('file://', '').split('/').filter(Boolean);
    return parts.slice(-2).join('/') || s;
  }

  function normContext(raw) {
    const M = FIELD_MAP.context;

    // AWR 不按段给 token 数，它给的是 selected_chunks：每块带 section 和 required。
    // 所以组成面板按 section 归并块数，并标出其中有几块是必需的——
    // 这是它真正提供的信息，不要编造每段的 token 数。
    const chunks = pick(raw, M.chunks, []) || [];
    const bySection = new Map();
    for (const c of chunks) {
      const name = String(c.section != null ? c.section : '未分段');
      const row = bySection.get(name) || { name, count: 0, required: 0 };
      row.count += 1;
      if (c.required) row.required += 1;
      bySection.set(name, row);
    }
    const sections = [...bySection.values()].sort((a, b) => b.count - a.count);

    const omissions = (pick(raw, M.omissions, []) || []).map((o) =>
      typeof o === 'string'
        ? { detail: o }
        : { detail: [o.key, o.section].filter((x) => x != null).join(' · ') || JSON.stringify(o), reason: o.reason }
    );

    // 完整性的各个维度。AWR 给的是一组布尔，缺哪一维要能一眼看到。
    const DIMS = [
      ['rules_complete', '规则'],
      ['goal_context_complete', '目标上下文'],
      ['work_state_complete', '工作状态'],
      ['acceptance_complete', '验收标准'],
      ['dependencies_complete', '依赖'],
      ['source_fresh', '源新鲜度'],
    ];
    const c = pick(raw, M.dimensions, {}) || {};
    const dimensions = DIMS
      .filter(([k]) => c[k] !== undefined)
      .map(([k, label]) => ({ key: k, label, ok: Boolean(c[k]) }));

    return {
      rendered: pick(raw, M.rendered, ''),
      sections,
      chunkTotal: chunks.length,
      dimensions,
      statusText: pick(raw, M.statusText, null),
      evidenceGaps: pick(raw, M.evidenceGaps, []) || [],
      unresolvedDeps: pick(raw, M.unresolvedDeps, []) || [],
      issues: pick(raw, M.issues, []) || [],
      total: pick(raw, M.total, null),
      requiredTokens: pick(raw, M.required, null),
      budget: pick(raw, M.budget, null),
      hash: pick(raw, M.hash, null),
      tokenizer: pick(raw, M.tokenizer, null),
      revision: pick(raw, M.revision, null),
      complete: pick(raw, M.complete, null),
      omissions,
    };
  }

  // ───────────────────────── 通用渲染块 ─────────────────────────

  function stateBlock(kind, title, msg, command) {
    const box = el('div', 'state' + (kind === 'err' ? ' err' : ''));
    box.appendChild(el('div', 'title', title));
    if (msg) box.appendChild(el('div', 'msg', msg));
    if (command) {
      const cmd = el('div', 'cmd');
      cmd.appendChild(el('span', 'prompt', '$'));
      cmd.appendChild(el('code', null, command));
      const btn = el('button', 'copy', '复制');
      btn.addEventListener('click', () => copyText(command, btn));
      cmd.appendChild(btn);
      box.appendChild(cmd);
    }
    return box;
  }

  function errorBlock(error, command) {
    const code = (error && error.code) || 'Error';
    const msg = (error && error.message) || '没有更多信息。';
    const advice = {
      SourceStale: '源文件改过了，AWR 的投影已经陈旧。去「索引源」点一次重新索引，再回来。',
      RevisionConflict: '项目状态在你操作期间变了。先刷新看一眼新状态，再决定下一步。',
      DemoMode: '当前是演示模式，下面显示的是内置样本数据。',
      BridgeUnreachable: '连不上本地桥接进程。确认 node server.js 还在跑。',
      NotJson: 'awr 返回的内容不是 JSON。展开下方「原始 JSON」看它到底输出了什么。',
      BudgetExceeded: '必需内容本身就超过了预算，AWR 拒绝给出残缺的上下文。把 budget 调大到必需量之上再编译。',
      OutcomeUnknown: '这条命令没有被终止，可能已经生效。先在终端里查一下当前状态，确认之后再决定要不要重跑——不要直接点重试。',
      BridgeTimeout: '只读命令超时已终止，重试是安全的。',
      ReindexNotAllowed: '重新索引默认关闭。用 --allow-reindex 重启桥接进程才能从界面触发。',
      OutputTooLarge: '输出太大，桥接不转发。请在终端里直接跑这条命令。',
      BridgeBusy: '同时在跑的命令太多，稍等一下再点。',
      ForbiddenHost: '请求的 Host 不是本机回环地址。请用 http://127.0.0.1:<端口> 打开。',
      ForbiddenOrigin: '请求来自别的源，已拒绝。',
      MissingGuardHeader: '状态变更请求缺少校验头，已拒绝。',
    }[code];

    const box = el('div', 'state err');
    const t = el('div', 'title');
    t.appendChild(el('span', 'errcode', code));
    box.appendChild(t);
    box.appendChild(el('div', 'msg', msg));
    if (advice) box.appendChild(el('div', 'msg', advice));
    if (command) {
      const cmd = el('div', 'cmd');
      cmd.appendChild(el('span', 'prompt', '$'));
      cmd.appendChild(el('code', null, command));
      const btn = el('button', 'copy', '复制');
      btn.addEventListener('click', () => copyText(command, btn));
      cmd.appendChild(btn);
      box.appendChild(cmd);
    }
    return box;
  }

  function showRaw(id, payload) {
    const node = $(id);
    if (node) node.textContent = JSON.stringify(payload, null, 2);
  }

  function copyText(text, btn) {
    const done = () => {
      if (!btn) return;
      const old = btn.textContent;
      btn.textContent = '已复制';
      setTimeout(() => { btn.textContent = old; }, 1400);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done, done);
    } else {
      const ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); } catch (_) {}
      ta.remove();
      done();
    }
  }

  // ───────────────────────── 概览 ─────────────────────────

  function renderOverview() {
    const s = state.status;
    const strip = $('statusStrip');
    clear(strip);
    if (!s) return;

    const cells = [
      { label: '进行中', value: s.currentTotal, hint: '有人正在做' },
      { label: '可开工', value: s.readyCount, hint: '依赖都满足了' },
      {
        label: '等待中',
        value: s.waitingCount != null ? s.waitingCount : '—',
        cls: s.waitingCount > 0 ? 'watch' : '',
        hint: s.waitingCount != null ? '在等回答或前置' : '这个 awr 版本不报告该队列',
      },
      { label: '被阻塞', value: s.blockedCount, cls: s.blockedCount > 0 ? 'alert' : '', hint: '卡住了，先看这里' },
      {
        label: '结构缺口',
        value: s.gapTotal != null ? s.gapTotal : '—',
        cls: s.gapTotal > 0 ? 'watch' : '',
        hint: s.orgState ? '组织状态 ' + s.orgState : '源结构的完整程度',
      },
      { label: 'Revision', value: s.revision != null ? s.revision : '—', dim: true, hint: '项目状态的版本号' },
    ];

    for (const c of cells) {
      const kv = el('div', 'kv');
      kv.appendChild(el('small', null, c.label));
      const b = el('b', [c.dim ? 'dim' : '', c.cls || ''].filter(Boolean).join(' '));
      b.textContent = typeof c.value === 'number' ? group(c.value) : c.value;
      kv.appendChild(b);
      kv.appendChild(el('div', 'kv-hint', c.hint));
      strip.appendChild(kv);
    }

    renderContextChart(s);
    renderQueueTabs();
    renderQueueList();
    renderGaps(s);
    renderPending(s);

    setText('navWorkCount', String(s.works.length || ''));
    setText('mcpCmd', `awr-mcp --project ${state.project}`);
    setText('mcpSub', state.mode === 'live' ? '本工具走 CLI，agent 走 MCP' : '演示模式');
  }

  function renderContextChart(s) {
    const wrap = $('cmpChart');
    clear(wrap);
    const sample = s.contextSample;

    if (!sample) {
      setText('heroBig', '—');
      setText('heroCap', '还没有编译记录');
      wrap.appendChild(stateBlock('empty', '还没有可对比的数据',
        '去「上下文」页编译一次，这里就会显示这个项目自己的体积对比。'));
      setText('cmpNote', '');
      setText('cmpSub', '');
      return;
    }

    const full = sample.full_corpus_tokens;
    const rows = [
      { label: '读完整源码', tokens: full, lead: false },
      { label: 'CLI JSON 全量响应', tokens: sample.json_dump_tokens, lead: false },
      { label: 'AWR 编译包', tokens: sample.compiled_tokens, lead: true },
    ];

    for (const r of rows) {
      const pct = full ? (r.tokens / full) * 100 : 0;
      const row = el('div', 'cmp-row' + (r.lead ? ' is-lead' : ''));
      const label = el('div', 'cmp-label');
      label.appendChild(document.createTextNode(r.label + ' '));
      label.appendChild(el('span', 'num', group(r.tokens) + ' tokens'));
      if (r.tokens !== full) {
        label.appendChild(el('span', 'delta', '−' + (100 - pct).toFixed(1) + '%'));
      }
      row.appendChild(label);
      const track = el('div', 'cmp-track');
      const fill = el('div', 'cmp-fill');
      fill.style.width = pct.toFixed(1) + '%';
      track.appendChild(fill);
      row.appendChild(track);
      wrap.appendChild(row);
    }

    const saved = full ? (100 - (sample.compiled_tokens / full) * 100).toFixed(1) : '0';
    setText('heroBig', '−' + saved + '%');
    setText('heroCap', '上下文 token 对比全量源码');
    setText('cmpSub', sample.note ? '公开基准值' : '本项目实测');
    setText('cmpNote', sample.note || '');
  }

  function renderQueueTabs() {
    const wrap = $('queueTabs');
    clear(wrap);
    for (const q of QUEUES) {
      const n = state.status.queues[q.key].total;
      const chip = el('button', 'chip');
      chip.setAttribute('aria-pressed', String(state.queueTab === q.key));
      chip.appendChild(document.createTextNode(q.label + ' '));
      chip.appendChild(el('b', null, String(n)));
      chip.addEventListener('click', () => {
        state.queueTab = q.key;
        renderQueueTabs();
        renderQueueList();
      });
      wrap.appendChild(chip);
    }
  }

  function renderQueueList() {
    const list = $('queueList');
    clear(list);
    const q = state.status.queues[state.queueTab];
    const meta = queueMeta(state.queueTab);

    setText('queueTitle', meta.label);
    setText('queueSub', q.total > q.items.length ? `显示 ${q.items.length} / ${q.total}` : `共 ${q.total}`);

    if (!q.available) {
      const li = el('li');
      li.style.gridTemplateColumns = '1fr';
      li.appendChild(stateBlock('empty', `这个版本的 awr 不报告${meta.label}队列`,
        '它是当前源码树里的能力，已发布的 0.4.0 还没有。装上带该能力的版本后这里会自动显示。'));
      list.appendChild(li);
      return;
    }

    if (!q.items.length) {
      const li = el('li');
      li.style.gridTemplateColumns = '1fr';
      li.appendChild(stateBlock('empty', `${meta.label}队列是空的`,
        state.queueTab === 'blocked' ? '没有被卡住的工作项，挺好。' : `当前没有${meta.label}的工作项。`));
      list.appendChild(li);
      return;
    }

    for (const w of q.items) {
      const li = el('li');
      li.appendChild(el('span', 'dot ' + meta.dot));

      const what = el('span', 'what');
      what.textContent = w.title;
      li.appendChild(what);

      const reason = w.blocker
        || (w.waitTotal ? `在等 ${w.waitTotal} 个回复` : '')
        || (w.codes.length ? w.codes.join(', ') : '')
        || w.nextAction
        || '';
      const meta2 = el('span', 'meta');
      meta2.textContent = w.key + (reason ? ' · ' + reason : '');
      li.appendChild(meta2);

      const age = el('span', 'age' + (state.queueTab === 'blocked' ? ' hot' : ''));
      age.textContent = w.owner || '';
      li.appendChild(age);

      li.style.cursor = 'pointer';
      li.addEventListener('click', () => {
        state.selectedWork = w.key;
        state.workFilter = 'all';
        go('work');
        renderWork();
      });
      list.appendChild(li);
    }
  }

  /**
   * 结构缺口。`awr status` 不返回 checkpoint 列表，但它返回 organization.gaps：
   * 源文件里缺目标、缺计划、缺任务结构的地方。这些是 agent 开不了工的真实原因。
   */
  function renderGaps(s) {
    const list = $('cpList');
    clear(list);

    setText('gapSub', s.gapTotal != null && s.gapTotal > s.gaps.length
      ? `显示 ${s.gaps.length} / ${s.gapTotal}`
      : `共 ${s.gaps.length}`);

    if (!s.gaps.length) {
      const li = el('li');
      li.style.gridTemplateColumns = '1fr';
      li.appendChild(stateBlock('empty', '没有结构缺口',
        '源文件里的目标、计划和任务结构都是齐的。'));
      list.appendChild(li);
      return;
    }

    for (const g of s.gaps.slice(0, 5)) {
      const li = el('li');
      li.appendChild(el('span', 'dot warn'));
      li.appendChild(el('span', 'what', g.detail || g.code || '（无说明）'));
      li.appendChild(el('span', 'meta', [g.code, g.target].filter(Boolean).join(' · ')));
      li.appendChild(el('span', 'age', ''));
      list.appendChild(li);
    }
  }

  /**
   * 待查的运行时操作。只有 status 真的报了才显示——发布版 0.4.0 不带这个字段，
   * 那就整块藏起来，不摆一个永远空的面板。
   */
  function renderPending(s) {
    const panel = $('pendingPanel');
    if (!s.pendingOps.length && !s.pendingTotal) {
      panel.hidden = true;
      return;
    }
    panel.hidden = false;
    setText('pendingSub', s.pendingTotal != null && s.pendingTotal > s.pendingOps.length
      ? `显示 ${s.pendingOps.length} / ${s.pendingTotal}`
      : `共 ${s.pendingOps.length}`);

    const list = $('pendingList');
    clear(list);
    for (const op of s.pendingOps.slice(0, 5)) {
      const li = el('li');
      li.appendChild(el('span', 'dot warn'));
      li.appendChild(el('span', 'what', op.code || '（无代码）'));
      li.appendChild(el('span', 'meta', [op.kind, op.id].filter(Boolean).join(' · ')));
      li.appendChild(el('span', 'age', ''));
      list.appendChild(li);
    }
  }

  // ───────────────────────── 工作项 ─────────────────────────

  function renderWork() {
    const s = state.status;
    if (!s) return;

    // 筛选芯片
    const filters = $('workFilters');
    clear(filters);
    const options = [{ key: 'all', label: '全部', count: s.works.length }].concat(
      QUEUES.map((q) => ({ key: q.key, label: q.label, count: s.queues[q.key].total }))
    );
    for (const o of options) {
      const chip = el('button', 'chip');
      chip.setAttribute('aria-pressed', String(state.workFilter === o.key));
      chip.appendChild(document.createTextNode(o.label + ' '));
      chip.appendChild(el('b', null, String(o.count)));
      chip.addEventListener('click', () => {
        state.workFilter = o.key;
        renderWork();
      });
      filters.appendChild(chip);
    }

    const rows = state.workFilter === 'all'
      ? s.works
      : s.works.filter((w) => w.queue === state.workFilter);

    const tbody = $('workRows');
    clear(tbody);
    clear($('workEmpty'));
    setText('workSub', `${rows.length} 项`);

    if (!rows.length) {
      $('workEmpty').appendChild(stateBlock('empty', '这个筛选下没有工作项', '换一个筛选看看。'));
      return;
    }

    if (!state.selectedWork || !rows.some((w) => w.key === state.selectedWork)) {
      state.selectedWork = rows[0].key;
    }

    for (const w of rows) {
      const tr = el('tr');
      tr.setAttribute('aria-selected', String(w.key === state.selectedWork));

      const c1 = el('td');
      c1.appendChild(el('span', 'id', w.key));
      tr.appendChild(c1);

      tr.appendChild(el('td', 'wide', w.title));

      const meta = queueMeta(w.queue);
      const c3 = el('td');
      const tagCls = { current: 'info', ready: 'ok', waiting: 'warn', blocked: 'crit' }[w.queue] || '';
      c3.appendChild(el('span', 'tag flat ' + tagCls, meta.label));
      tr.appendChild(c3);

      tr.appendChild(el('td', null, w.rawStatus || w.status || '—'));
      tr.appendChild(el('td', null, w.owner || '—'));
      const claimed = w.raw && Array.isArray(w.raw.claims) && w.raw.claims.length;
      tr.appendChild(el('td', null, claimed ? '已认领' : (w.raw && w.raw.ownership_required ? '需认领' : '—')));
      tr.appendChild(el('td', null, w.codes.length ? w.codes.join(', ') : '—'));
      tr.appendChild(el('td', 'num', w.revision != null ? String(w.revision) : '—'));

      tr.addEventListener('click', () => {
        state.selectedWork = w.key;
        renderWork();
      });
      tbody.appendChild(tr);
    }

    renderWorkDetail(state.selectedWork);
  }

  async function renderWorkDetail(key) {
    const token = detailGuard.begin(key);
    const box = $('workDetail');
    clear(box);
    setText('detailId', key || '详情');

    // 缓存的是 { detail, raw } 一对，不是只有规范化后的详情。
    // 只缓存详情的话，命中缓存时原始 JSON 面板还停在上一个工作项的响应上——
    // 显示的是 A 的内容，配的却是 B 的响应。
    let entry = state.workDetail[key];

    if (!entry) {
      box.appendChild(el('div', 'skeleton'));
      let raw = null;
      let detail = null;

      if (state.mode === 'demo') {
        raw = { ok: true, data: window.AWR_DEMO.workShow(key), note: '演示数据' };
        if (!detailGuard.isCurrent(token)) return;
        detail = normWorkDetail(raw.data);
      } else {
        const res = await callApi('/api/work?key=' + encodeURIComponent(key));
        // 回来晚了就整条丢掉：不写 state.raw.work、不画面板、不报错。
        // 成功和失败一视同仁，否则一个迟到的失败会盖掉当前选中项的正常内容。
        if (!detailGuard.isCurrent(token)) return;
        if (!res.ok) {
          // 失败不进缓存，但它的原始响应要显示出来——那正是排查用的东西。
          state.raw.work = res;
          showRaw('rawWorkBody', res);
          clear(box);
          box.appendChild(errorBlock(res.error, res.command));
          return;
        }
        raw = res;
        detail = normWorkDetail(res.data);
      }

      entry = { detail, raw };
      if (detail) state.workDetail[key] = entry;
    }

    if (!detailGuard.isCurrent(token)) return;

    // 命中缓存也要把原始响应一起恢复，两者始终配套。
    state.raw.work = entry.raw;
    showRaw('rawWorkBody', entry.raw);

    const detail = entry.detail;
    clear(box);
    if (!detail) {
      box.appendChild(stateBlock('empty', '没有这个工作项的详情', '它可能只出现在队列里，源文件中没有完整定义。'));
      return;
    }

    setText('detailStatus', [queueMeta(detail.queue).label, detail.status].filter(Boolean).join(' · '));

    box.appendChild(el('h3', null, detail.title));

    if (detail.goal) {
      const sec = el('div');
      sec.appendChild(el('h4', null, '目标'));
      sec.appendChild(el('p', 'quote', detail.goal));
      box.appendChild(sec);
    }

    if (detail.acceptance.length) {
      const sec = el('div');
      sec.appendChild(el('h4', null, `验收标准（${detail.acceptance.length} 条，逐字取自源文件）`));
      const ul = el('ul', 'crit-list');
      for (const a of detail.acceptance) {
        const li = el('li');
        li.appendChild(el('span', 'box', '·'));
        li.appendChild(el('span', null, a.criterion));
        ul.appendChild(li);
      }
      sec.appendChild(ul);
      sec.appendChild(el('p', 'figure-note',
        'AWR 不在这里记「达成没达成」。是否达成是在 complete 的时候逐条对证据验的。'));
      box.appendChild(sec);
    }

    const blockText = detail.blocker
      || (detail.waitTotal ? `在等 ${detail.waitTotal} 个用户回复` : null);
    if (blockText) {
      const sec = el('div');
      sec.appendChild(el('h4', null, detail.blocker ? '卡在哪' : '在等什么'));
      sec.appendChild(el('p', 'quote', blockText));
      box.appendChild(sec);
    }

    if (detail.dependsOn.length || detail.missingDeps.length) {
      const sec = el('div');
      sec.appendChild(el('h4', null, '依赖'));
      const parts = detail.dependsOn.map((d) => d.status ? `${d.key}（${d.status}）` : d.key);
      if (parts.length) sec.appendChild(el('p', 'quote', parts.join('、')));
      if (detail.missingDeps.length) {
        sec.appendChild(el('p', 'quote', '源文件里找不到：' + detail.missingDeps.join('、')));
      }
      box.appendChild(sec);
    }

    if (detail.claims.length) {
      const sec = el('div');
      sec.appendChild(el('h4', null, '谁占着这件活'));
      const ul = el('ul', 'crit-list');
      for (const c of detail.claims) {
        const li = el('li');
        li.appendChild(el('span', 'box done', '●'));
        const txt = el('span');
        txt.textContent = [c.agent && `agent ${c.agent}`, c.session && `session ${c.session}`]
          .filter(Boolean).join(' · ') || '（无标识）';
        if (c.expiresAt) {
          txt.appendChild(document.createTextNode(' '));
          txt.appendChild(el('span', 'id', until(c.expiresAt)));
        }
        li.appendChild(txt);
        ul.appendChild(li);
      }
      sec.appendChild(ul);
      sec.appendChild(el('p', 'figure-note',
        'claim 是 AWR 的所有权凭证。别的 session 要动这件活，得先等它释放或显式接手。'));
      box.appendChild(sec);
    }

    if (detail.evidence.length || detail.decisions.length) {
      const sec = el('div');
      sec.appendChild(el('h4', null, `证据与决策（${detail.evidence.length} 份证据 · ${detail.decisions.length} 条决策）`));
      const ul = el('ul', 'crit-list');
      for (const e of detail.evidence.slice(0, 8)) {
        const li = el('li');
        li.appendChild(el('span', 'box done', '✓'));
        li.appendChild(el('span', null,
          [e.external_key || e.key, e.evidence_type || e.kind, e.level].filter(Boolean).join(' · ')));
        ul.appendChild(li);
      }
      for (const d of detail.decisions.slice(0, 8)) {
        const li = el('li');
        li.appendChild(el('span', 'box', '§'));
        li.appendChild(el('span', null, d.title || d.external_key || JSON.stringify(d).slice(0, 80)));
        ul.appendChild(li);
      }
      sec.appendChild(ul);
      box.appendChild(sec);
    }

    if (detail.diagnostics.length || detail.cycles.length) {
      const sec = el('div');
      sec.appendChild(el('h4', null, '诊断'));
      const codes = detail.diagnostics
        .map((d) => (typeof d === 'string' ? d : d.code))
        .filter(Boolean);
      if (codes.length) sec.appendChild(el('p', 'quote', codes.join('、')));
      if (detail.cycles.length) {
        sec.appendChild(el('p', 'quote', '依赖成环：' + detail.cycles.join(' → ')));
      }
      box.appendChild(sec);
    }

    const sec = el('div');
    sec.appendChild(el('h4', null, '下一步'));
    sec.appendChild(el('p', 'quote', detail.nextAction || '（源文件里没写）'));
    const act = el('div', 'actions');
    const btn = el('button', 'btn', '为这一项编译上下文');
    btn.addEventListener('click', () => {
      $('fWork').value = detail.key;
      if (detail.goal) {
        const m = String(detail.goal).match(/goal#[\w.-]+/);
        if (m) $('fGoal').value = m[0];
      }
      go('context');
      updateCliMirror();
    });
    act.appendChild(btn);
    sec.appendChild(act);
    box.appendChild(sec);
  }

  // ───────────────────────── 上下文 ─────────────────────────

  function fillWorkSelect() {
    const sel = $('fWork');
    const keep = sel.value;
    clear(sel);
    for (const w of state.status ? state.status.works : []) {
      const o = el('option', null, `${w.key} — ${w.title}`);
      o.value = w.key;
      sel.appendChild(o);
    }
    if (keep) sel.value = keep;
    updateCliMirror();
  }

  function updateCliMirror() {
    const work = $('fWork').value;
    const goal = $('fGoal').value.trim();
    const budget = $('fBudget').value;
    const intent = $('fIntent').value.trim();
    let cmd = `awr --project ${state.project} --json context compile --work ${work || '<WORK>'}`;
    if (goal) cmd += ` --goal '${goal}'`;
    cmd += ` --budget ${budget}`;
    if (intent) cmd += ` --intent '${intent}'`;
    setText('cliMirror', cmd);
  }

  async function doCompile() {
    const btn = $('compileBtn');
    btn.disabled = true;
    setText('compileHint', '编译中…');

    const work = $('fWork').value;
    const goal = $('fGoal').value.trim();
    const budget = Number($('fBudget').value);
    const intent = $('fIntent').value.trim();

    let ctx = null;
    let failure = null;

    if (state.mode === 'demo') {
      await new Promise((r) => setTimeout(r, 260));
      const raw = window.AWR_DEMO.compile(work, budget);
      state.raw.context = { ok: true, data: raw };
      showRaw('rawContextBody', state.raw.context);
      ctx = normContext(raw);
    } else {
      const res = await callApi('/api/context/compile', {
        method: 'POST',
        body: JSON.stringify({ work, goal, budget, intent }),
      });
      state.raw.context = res;
      showRaw('rawContextBody', res);
      if (res.ok) ctx = normContext(res.data);
      else failure = res;
    }

    btn.disabled = false;
    state.compile = ctx;

    if (failure) {
      clear($('breakdown'));
      $('breakdown').appendChild(errorBlock(failure.error, failure.command));
      clear($('completeBody'));
      setText('packetTotal', '');
      setText('packetNote', '');
      setText('packetPreview', '');
      setText('compileHint', '编译失败。');
      return;
    }

    renderCompile();
    setText('compileHint', ctx.revision != null
      ? `revision ${ctx.revision} · 不写权威源，可能刷新投影`
      : '不写权威源，可能刷新投影');
  }

  function renderCompile() {
    const ctx = state.compile;
    const bd = $('breakdown');
    clear(bd);
    if (!ctx) {
      bd.appendChild(stateBlock('empty', '还没有编译', '在上面选好参数，点「编译」。'));
      clear($('completeBody'));
      $('completeBody').appendChild(stateBlock('empty', '—', '编译之后这里会显示有没有内容被省略。'));
      setText('packetPreview', '');
      return;
    }

    const max = Math.max.apply(null, ctx.sections.map((s) => s.count).concat([1]));
    for (const s of ctx.sections) {
      const row = el('div', 'bd-row');
      row.appendChild(el('div', 'bd-name', s.name));
      row.appendChild(el('div', 'bd-val', s.required ? `${s.count} 块（${s.required} 必需）` : `${s.count} 块`));
      const track = el('div', 'bd-track');
      const fill = el('div', 'bd-fill');
      fill.style.width = ((s.count / max) * 100).toFixed(1) + '%';
      track.appendChild(fill);
      row.appendChild(track);
      bd.appendChild(row);
    }

    const over = ctx.budget != null && ctx.total > ctx.budget;
    setText('packetTotal', `${group(ctx.total)} / ${group(ctx.budget)} tokens`);
    // AWR 只给整体 token 数和每块的 section/required，不给每段的 token 数——
    // 所以这里量的是块数，不编造每段占了多少 token。
    setText('packetNote', over
      ? '总量超过了预算，AWR 会省略非必需的块来塞进去。'
      : `共 ${ctx.chunkTotal} 块。条形量的是块数；AWR 只给整体 token 数，不给每段的。`);

    // 完整性
    const cb = $('completeBody');
    clear(cb);
    const ok = ctx.complete !== false;
    setText('completeSub', ok ? '完整' : '有省略');

    const head = el('div', 'loops');
    const li = el('li');
    li.appendChild(el('span', 'dot ' + (ok ? 'ok' : 'warn')));
    li.appendChild(el('span', 'what', ctx.statusText || (ok ? '上下文完整' : '上下文不完整')));
    li.appendChild(el('span', 'meta', ctx.requiredTokens != null ? `必需内容 ${group(ctx.requiredTokens)} tokens` : ''));
    head.appendChild(li);
    cb.appendChild(head);

    // 分维度：AWR 是逐项判定的，缺哪一维要能一眼看到。
    if (ctx.dimensions.length) {
      const chips = el('div', 'chips');
      chips.style.marginTop = '14px';
      for (const d of ctx.dimensions) {
        const chip = el('span', 'tag flat ' + (d.ok ? 'ok' : 'crit'), d.label);
        chips.appendChild(chip);
      }
      cb.appendChild(chips);
    }

    // 证据缺口：哪条验收标准还没有证据兜底。
    if (ctx.evidenceGaps.length) {
      const h = el('p', 'figure-note');
      h.style.marginBottom = '6px';
      h.textContent = `证据缺口 ${ctx.evidenceGaps.length} 项——完成这件活之前每条验收标准都要对上证据：`;
      cb.appendChild(h);
      const ul = el('ul', 'crit-list');
      for (const g of ctx.evidenceGaps.slice(0, 6)) {
        const item = el('li');
        item.appendChild(el('span', 'box', '!'));
        item.appendChild(el('span', null,
          typeof g === 'string' ? g : [g.reference, g.reason || g.code].filter(Boolean).join('：')));
        ul.appendChild(item);
      }
      cb.appendChild(ul);
    }

    if (ctx.unresolvedDeps.length) {
      const ul = el('ul', 'crit-list');
      ul.style.marginTop = '10px';
      for (const d of ctx.unresolvedDeps.slice(0, 6)) {
        const item = el('li');
        item.appendChild(el('span', 'box', '⛔'));
        item.appendChild(el('span', null, '未决依赖：' + (typeof d === 'string' ? d : (d.external_key || JSON.stringify(d)))));
        ul.appendChild(item);
      }
      cb.appendChild(ul);
    }

    if (ctx.issues.length) {
      const ul = el('ul', 'crit-list');
      ul.style.marginTop = '10px';
      for (const i of ctx.issues.slice(0, 6)) {
        const item = el('li');
        item.appendChild(el('span', 'box', '!'));
        item.appendChild(el('span', null, typeof i === 'string' ? i : (i.detail || i.code || JSON.stringify(i))));
        ul.appendChild(item);
      }
      cb.appendChild(ul);
    }

    if (ctx.omissions.length) {
      const ul = el('ul', 'crit-list');
      ul.style.marginTop = '14px';
      for (const o of ctx.omissions) {
        const item = el('li');
        item.appendChild(el('span', 'box', '—'));
        item.appendChild(el('span', null, o.reason ? `${o.detail}（${o.reason}）` : o.detail));
        ul.appendChild(item);
      }
      cb.appendChild(ul);
      const tip = el('p', 'figure-note', '把 budget 调大再编译一次，就能把这些装回去。');
      cb.appendChild(tip);
    }

    setText('packetPreview', ctx.rendered || '（这次编译没有返回渲染文本）');
  }

  // ───────────────────────── 索引源 ─────────────────────────

  function renderSources() {
    const data = state.sources;
    const tbody = $('srcRows');
    clear(tbody);
    clear($('srcEmpty'));
    if (!data) return;

    setText('srcCmd', `awr --project ${state.project} --json intake inspect`);

    const files = data.files;
    const stale = files.filter((f) => f.state !== 'fresh' && f.state !== 'indexed').length;

    setText('srcTitle', `${files.length} 个源`);
    setText('srcSub', [
      data.summary ? Object.keys(data.summary).map((k) => `${data.summary[k]} ${k}`).join(' · ') : '',
      stale ? `${stale} 个不新鲜` : '',
      data.issues.length ? `${data.issues.length} 个问题` : '',
    ].filter(Boolean).join(' · '));
    setText('navSourceCount', String(files.length || ''));

    if (!files.length) {
      $('srcEmpty').appendChild(stateBlock('empty', 'manifest 没匹配到任何文件',
        '检查 project.toml 里的 [[sources]] 路径写对了没有。',
        `awr --project ${state.project} intake inspect`));
      return;
    }

    const tagFor = { fresh: 'ok', indexed: 'ok', stale: 'warn', drift: 'warn', new: 'info', rejected: 'crit' };

    for (const f of files) {
      const tr = el('tr');
      tr.appendChild(el('td', 'wide', f.path));
      tr.appendChild(el('td', null, f.kind));
      tr.appendChild(el('td', null, f.role || '—'));

      const c4 = el('td');
      c4.appendChild(el('span', 'tag flat ' + (tagFor[f.state] || 'warn'), f.state));
      tr.appendChild(c4);

      tr.appendChild(el('td', 'num', f.revision != null ? String(f.revision) : '—'));
      tbody.appendChild(tr);

      if (f.rejection) {
        const exp = el('tr', 'expando');
        exp.hidden = true;
        const td = el('td');
        td.colSpan = 5;
        const r = f.rejection;
        const loc = r.location
          ? (r.location.line != null ? `第 ${r.location.line} 行` + (r.location.column != null ? `，第 ${r.location.column} 列` : '') : '位置不可用')
          : '位置不可用';
        td.appendChild(el('div', null, `规则：${r.rule || '—'}`));
        td.appendChild(el('div', null, `位置：${loc}`));
        td.appendChild(el('div', null, `怎么修：${r.repair || '—'}`));
        const note = el('p', 'figure-note', 'AWR 不会回显匹配到的原值和上下文，本工具也不会自己去读源文件补出来。');
        td.appendChild(note);
        exp.appendChild(td);
        tbody.appendChild(exp);

        tr.addEventListener('click', () => { exp.hidden = !exp.hidden; });
        tr.title = '点一下看被拒的原因';
      }
    }
  }

  async function doReindex() {
    const btn = $('reindexBtn');

    // 这是界面上唯一会改动 AWR 状态的操作，明确确认一次。
    const okToRun = window.confirm(
      '重新索引会刷新 AWR 的源投影，并推进项目 revision。\n\n' +
      '它不会改动你的 Markdown / YAML 源文件。\n\n要继续吗？'
    );
    if (!okToRun) return;

    btn.disabled = true;
    const old = btn.textContent;
    btn.textContent = '索引中…';

    if (state.mode === 'demo') {
      await new Promise((r) => setTimeout(r, 500));
      btn.disabled = false;
      btn.textContent = old;
      clear($('srcEmpty'));
      $('srcEmpty').appendChild(stateBlock('empty', '演示模式不会真的索引',
        '连上真实项目后，这个按钮会跑下面这条命令。',
        `awr --project <项目目录> source reindex`));
      return;
    }

    const res = await callApi('/api/source/reindex', { method: 'POST' });
    btn.disabled = false;
    btn.textContent = old;

    if (!res.ok) {
      clear($('srcEmpty'));
      $('srcEmpty').appendChild(errorBlock(res.error, res.command));
      return;
    }
    // 索引推进了 revision，所有缓存作废，整页重来。
    detailGuard.invalidate();
    state.workDetail = {};
    state.compile = null;
    await loadAll();
  }

  // ───────────────────────── 加载 ─────────────────────────

  async function loadAll() {
    const health = await callApi('/api/health');
    if (health.ok) {
      state.mode = health.data.mode;
      // 演示模式下显示样本项目名，而不是本工具自己所在的那个目录——
      // 那个路径会让人以为它真的在读这个目录。
      state.project = state.mode === 'demo' ? '.local/demo' : (health.data.project || '.');
      state.reason = health.data.reason;
      setText('verTag', health.data.awrVersion || 'v0.4.0');
    } else {
      state.mode = 'demo';
      state.reason = '连不上本地桥接进程（node server.js）。现在显示的是内置样本数据。';
      state.project = '.local/demo';
    }

    setText('projPath', state.project);
    renderModeUi();

    if (state.mode === 'demo') {
      state.status = normStatus(window.AWR_DEMO.status, null);
      state.sources = normSources(window.AWR_DEMO.sources);
      state.raw.overview = { ok: true, data: window.AWR_DEMO.status, note: '演示数据' };
      state.raw.sources = { ok: true, data: window.AWR_DEMO.sources, note: '演示数据' };
    } else {
      // 发布版 0.4.0 的 status 不带 ready/blocked 列表，所以两条一起拉。
      const [st, rdy, src] = await Promise.all([
        callApi('/api/status'), callApi('/api/ready'), callApi('/api/sources'),
      ]);
      state.raw.overview = { status: st, ready: rdy };
      state.raw.sources = src;

      if (st.ok) {
        state.status = normStatus(st.data, rdy.ok ? rdy.data : null);
      } else {
        state.status = null;
        clear($('statusStrip'));
        $('statusStrip').appendChild(errorBlock(st.error, st.command));
      }
      state.sources = src.ok ? normSources(src.data) : null;
      if (!src.ok) {
        clear($('srcEmpty'));
        $('srcEmpty').appendChild(errorBlock(src.error, src.command));
      }
    }

    showRaw('rawOverviewBody', state.raw.overview);
    showRaw('rawSourcesBody', state.raw.sources);

    if (state.status) {
      renderOverview();
      renderWork();
      fillWorkSelect();
    }
    if (state.sources) renderSources();
    renderCompile();

    // 顶栏的新鲜度标签要读 status，所以在 status 到手之后再渲染一次。
    renderModeUi();
  }

  function renderModeUi() {
    const demo = state.mode === 'demo';
    $('tagDemo').hidden = !demo;
    $('tagBackend').hidden = demo;
    if (!demo) $('tagBackend').textContent = 'CLI 已连接';

    const fresh = $('tagFresh');
    if (state.status && state.status.lastIndexed) {
      fresh.hidden = false;
      const drift = state.status.driftCount;
      fresh.className = 'tag ' + (drift ? 'warn' : 'ok live');
      fresh.textContent = drift ? `${drift} 个文件已漂移` : `索引 ${since(state.status.lastIndexed)}`;
    } else {
      fresh.hidden = true;
    }

    setText('footMode', demo ? '演示模式 · 数据是编的' : `真实数据 · ${state.project}`);

    const banner = $('modeBanner');
    if (demo && !sessionStorage.getItem('awr.banner.hidden')) {
      banner.hidden = false;
      banner.className = 'banner';
      const text = $('modeBannerText');
      clear(text);
      const b = el('b', null, '现在看到的是演示数据。');
      text.appendChild(b);
      text.appendChild(document.createTextNode(' ' + (state.reason || '') + ' 要看你自己的项目：装好 awr 之后，用 '));
      text.appendChild(el('code', null, 'node server.js --project /你的/项目路径'));
      text.appendChild(document.createTextNode(' 重新启动。界面用法完全一样，可以先在这里随便点。'));
    } else {
      banner.hidden = true;
    }
  }

  // ───────────────────────── 导航 ─────────────────────────

  const VIEWS = ['overview', 'work', 'context', 'sources'];

  function go(view) {
    if (VIEWS.indexOf(view) < 0) view = 'overview';
    state.view = view;
    for (const v of VIEWS) $('view-' + v).hidden = v !== view;
    for (const a of document.querySelectorAll('.rail a')) {
      a.setAttribute('aria-current', String(a.dataset.view === view));
    }
    history.replaceState(null, '', '#' + view);
    window.scrollTo({ top: 0 });
  }

  // ───────────────────────── 新手引导 ─────────────────────────

  const TOUR = [
    {
      title: '这个工具是干什么的',
      html: [
        '<p>你的 coding agent 每开一个新会话，都得先搞清楚「这个项目在干嘛、我该接着做什么」。AWR 就是替它记住这些事的那一层。</p>',
        '<p>AWR Inspector 是给<b>人</b>看的那一面：agent 看到的状态，你也能看到同一份。</p>',
        '<div class="tour-art"><div class="row"><span>源文件</span><span class="bar on"></span></div><div class="row"><span>AWR 索引</span><span class="bar on"></span></div><div class="row"><span>上下文包</span><span class="bar on bar-short"></span></div></div>',
      ].join(''),
    },
    {
      title: '第一站：概览',
      html: [
        '<p>四个队列告诉你所有工作项现在的处境：<b>进行中</b>、<b>可开工</b>、<b>等待中</b>、<b>被阻塞</b>。</p>',
        '<p>每天打开先看后两个——进度停下来的地方都在那儿。点任何一条都能跳到详情。</p>',
      ].join(''),
    },
    {
      title: '第二站：工作项',
      html: [
        '<p>一件活的全部信息：目标、验收标准、卡在哪、下一步。</p>',
        '<p>验收标准是<b>逐字</b>从你的 Markdown 抄过来的，不是另写的摘要——AWR 完成任务时要靠它逐条对上证据。</p>',
      ].join(''),
    },
    {
      title: '第三站：上下文',
      html: [
        '<p>这里能亲手编译一个上下文包，看清楚 token 花在了哪几段，以及有没有东西因为预算被省略掉。</p>',
        '<p>编译不写权威源、也不推进工作状态；但它可能刷新本地投影（<code>source_refresh_performed</code>）。</p>',
      ].join(''),
    },
    {
      title: '最后：每条命令都能自己跑',
      html: [
        '<p>界面上每个 <code>$</code> 开头的框，都是它后台真正执行的那条命令。复制到终端跑，结果一模一样。</p>',
        '<p>看不懂某个词？点右上角的<b>「术语」</b>。每个面板标题旁边的 <b>?</b> 会用大白话解释这一块在说什么。</p>',
      ].join(''),
    },
  ];

  let tourStep = 0;

  function openTour(step) {
    tourStep = step || 0;
    renderTour();
    $('tour').hidden = false;
  }

  function renderTour() {
    const s = TOUR[tourStep];
    setText('tourTitle', `${tourStep + 1}/${TOUR.length} · ${s.title}`);
    $('tourBody').innerHTML = s.html;
    setText('tourNext', tourStep === TOUR.length - 1 ? '开始使用' : '下一步');
    const dots = $('tourDots');
    clear(dots);
    TOUR.forEach((_, i) => {
      const d = el('i');
      if (i === tourStep) d.className = 'on';
      dots.appendChild(d);
    });
  }

  function closeTour() {
    $('tour').hidden = true;
    try { localStorage.setItem('awr.tour.seen', '1'); } catch (_) {}
  }

  // ───────────────────────── 启动 ─────────────────────────

  function wire() {
    for (const a of document.querySelectorAll('.rail a')) {
      a.addEventListener('click', (e) => {
        e.preventDefault();
        go(a.dataset.view);
      });
    }

    // 「这是什么」问号
    for (const b of document.querySelectorAll('.why')) {
      b.addEventListener('click', () => {
        const note = document.querySelector(`[data-note="${b.dataset.why}"]`);
        if (note) note.hidden = !note.hidden;
      });
    }

    // 复制按钮
    for (const b of document.querySelectorAll('.copy[data-copy-target]')) {
      b.addEventListener('click', () => {
        const target = $(b.dataset.copyTarget);
        if (target) copyText(target.textContent, b);
      });
    }

    $('btnRefresh').addEventListener('click', async () => {
      const b = $('btnRefresh');
      b.classList.add('spin');
      // 在途的详情请求全部作废，免得旧数据在刷新后落地。
      detailGuard.invalidate();
      state.workDetail = {};
      await loadAll();
      b.classList.remove('spin');
    });

    $('btnGuide').addEventListener('click', () => openTour(0));
    $('tourNext').addEventListener('click', () => {
      if (tourStep === TOUR.length - 1) closeTour();
      else { tourStep++; renderTour(); }
    });
    $('tourSkip').addEventListener('click', closeTour);
    $('tour').addEventListener('click', (e) => { if (e.target === $('tour')) closeTour(); });

    $('btnGloss').addEventListener('click', () => { $('glossary').hidden = false; });
    $('glossClose').addEventListener('click', () => { $('glossary').hidden = true; });
    $('glossary').addEventListener('click', (e) => { if (e.target === $('glossary')) $('glossary').hidden = true; });

    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        $('tour').hidden = true;
        $('glossary').hidden = true;
      }
    });

    $('modeBannerClose').addEventListener('click', () => {
      $('modeBanner').hidden = true;
      try { sessionStorage.setItem('awr.banner.hidden', '1'); } catch (_) {}
    });

    $('btnTheme').addEventListener('click', () => {
      const cur = document.documentElement.getAttribute('data-theme');
      const isDark = cur ? cur === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
      const next = isDark ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', next);
      try { localStorage.setItem('awr.theme', next); } catch (_) {}
    });

    for (const id of ['fWork', 'fGoal', 'fBudget', 'fIntent']) {
      $(id).addEventListener('input', updateCliMirror);
      $(id).addEventListener('change', updateCliMirror);
    }
    $('compileBtn').addEventListener('click', doCompile);
    $('reindexBtn').addEventListener('click', doReindex);
  }

  function restoreTheme() {
    try {
      const saved = localStorage.getItem('awr.theme');
      if (saved) document.documentElement.setAttribute('data-theme', saved);
    } catch (_) {}
  }

  async function boot() {
    restoreTheme();
    wire();
    go((location.hash || '#overview').slice(1));
    await loadAll();

    let seen = null;
    try { seen = localStorage.getItem('awr.tour.seen'); } catch (_) {}
    if (!seen) openTour(0);
  }

  if (typeof document !== 'undefined') {
    document.addEventListener('DOMContentLoaded', boot);
  }

  // 给测试用。浏览器里没有 module，这一段不执行。
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { createGenerationGuard, state, detailGuard, renderWorkDetail };
  }
})();
