/**
 * Team Web collaboration loop (WS-044).
 *
 * Interactive My Projects → overview / card|list / detail surface with
 * collaboration writes. Talks to the Inspector bridge `/api/team/*`, which
 * either serves fixtures (demo) or proxies the designed awr-server `/v1/web`
 * cookie entry. Bearer tokens never stay in page JS after login handoff.
 */
(function (root) {
  'use strict';

  const GUARD = 'X-AWR-Inspector';

  function el(tag, attrs, text) {
    const node = document.createElement(tag);
    if (attrs) {
      for (const [k, v] of Object.entries(attrs)) {
        if (k === 'class') node.className = v;
        else if (k === 'dataset') {
          for (const [dk, dv] of Object.entries(v)) node.dataset[dk] = dv;
        } else if (v != null) node.setAttribute(k, v);
      }
    }
    if (text != null) node.textContent = text;
    return node;
  }

  function t(i18n, key, vars) {
    try {
      return i18n.t(key, vars);
    } catch (_) {
      return key;
    }
  }

  function createTeamWeb(opts) {
    const i18n = opts.i18n;
    const $ = opts.$;
    const network = root.AWR_TEAM_NETWORK || (typeof require === 'function' ? require('./team-network') : null);
    const state = {
      viewMode: 'team', // personal | team
      layout: 'cards', // collaboration graph | list
      projectKey: null,
      projects: [],
      works: [],
      streams: [],
      graphLoading: false,
      selected: null,
      session: null,
      disconnect: false,
      inflight: Object.create(null),
      lastReceipts: Object.create(null),
      members: [],
      raw: null,
      error: null,
      loading: false,
      detailLoading: false,
      detailResponse: null,
      connectOpen: false,
      handoffOpen: Object.create(null),
    };
    let generation = 0;
    let detailGeneration = 0;
    let loginForm = null;
    let loginInput = null;
    let net = null;
    let pendingDetails = new Map();

    function clearProjectData() {
      state.projects = [];
      state.projectKey = null;
      state.works = [];
      state.streams = [];
      state.graphLoading = false;
      pendingDetails = new Map();
      state.members = [];
      state.selected = null;
      state.raw = null;
      state.detailLoading = false;
      state.detailResponse = null;
      state.lastReceipts = Object.create(null);
      state.connectOpen = false;
      state.handoffOpen = Object.create(null);
    }

    function failed(body) {
      state.error = (body && body.error) || { code: 'InvalidResponse', message: 'Invalid Team response' };
      state.disconnect = state.error.code === 'BridgeUnreachable';
      if (['Unauthenticated', 'SessionExpired'].includes(state.error.code)) {
        ++generation;
        ++detailGeneration;
        state.session = null;
        clearProjectData();
        render();
      }
      return body;
    }

    async function api(path, options) {
      const opts2 = Object.assign({ credentials: 'same-origin' }, options || {});
      opts2.headers = Object.assign(
        { 'content-type': 'application/json', [GUARD]: '1' },
        opts2.headers || {}
      );
      try {
        const res = await fetch(path, opts2);
        const body = await res.json();
        return res.ok === false && !body.error
          ? { ok: false, error: { code: body.code || 'RequestFailed', message: body.message || `HTTP ${res.status}` } }
          : body;
      } catch (err) {
        return {
          ok: false,
          error: { code: 'BridgeUnreachable', message: String(err && err.message) },
        };
      }
    }

    function renderProjects(host) {
      clear(host);
      host.appendChild(el('h3', null, t(i18n, 'ui.my_projects')));
      if (!state.projects.length) {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, state.loading ? 'ui.team_loading' : 'ui.no_projects_yet')));
        return;
      }
      const picker = el('select', { id: 'teamProjectSelect', 'aria-label': t(i18n, 'ui.my_projects') });
      for (const p of state.projects) {
        const option = el('option', { value: p.key }, p.title || p.key);
        option.value = p.key;
        picker.appendChild(option);
      }
      picker.value = state.projectKey;
      picker.addEventListener('change', () => {
        state.projectKey = picker.value;
        state.selected = null;
        refresh();
      });
      host.appendChild(picker);
    }

    // The website task-node structure; project data remains text, never HTML.
    function workCard(w, lane) {
      const card = el('article', {
        class: 'task-node', dataset: { key: w.key, status: network.visualStatus(w) },
        tabindex: '0', role: 'button', 'aria-label': w.title || w.key,
        'aria-pressed': String(state.selected === w.key), 'aria-controls': 'teamDetail',
      });
      card.appendChild(el('span', { class: 'node-status', 'aria-hidden': 'true' }));
      const body = el('span', { class: 'node-body' });
      const top = el('span', { class: 'node-top' });
      top.appendChild(el('b', { class: 'node-id' }, w.key));
      top.appendChild(el('span', { class: 'task-state' }, workStatus(w)));
      body.appendChild(top);
      body.appendChild(el('strong', null, w.title || w.key));
      const people = el('span', { class: 'node-people' });
      people.appendChild(el('span', { class: 'node-owner' }, w.owner_person || t(i18n, 'ui.network_owner_unknown')));
      const agent = w.agent && typeof w.agent === 'object' ? w.agent.id : w.agent;
      if (agent) people.appendChild(el('span', { class: 'node-agent' }, [agent, w.model].filter(Boolean).join(' · ')));
      if (lane) people.appendChild(el('span', { class: 'node-lane' }, lane.name));
      body.appendChild(people);
      if (w.dependency_export_unavailable) body.appendChild(el('span', { class: 'node-note' }, t(i18n, 'ui.network_dependency_gap')));
      card.appendChild(body);
      card.addEventListener('click', () => selectWork(w.key));
      card.addEventListener('keydown', (event) => {
        if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); selectWork(w.key); }
      });
      return card;
    }

    function workRow(w) {
      const tr = el('tr', {
        class: state.selected === w.key ? 'selected' : '',
        dataset: { key: w.key },
      });
      const blocker =
        w.blocker && typeof w.blocker === 'object' ? w.blocker.summary : w.blocker;
      const agent =
        w.agent && typeof w.agent === 'object' ? w.agent.id || '' : w.agent || '';
      for (const text of [
        w.key,
        w.title,
        w.owner_person || '',
        agent,
        w.outcome || '',
        blocker || '',
        w.next_step || '',
      ]) {
        const td = el('td');
        if (tr.children.length === 0) {
          const select = el('button', { class: 'linkish', type: 'button' }, text);
          select.addEventListener('click', (event) => { event.stopPropagation(); return selectWork(w.key); });
          td.appendChild(select);
        } else td.textContent = text || '—';
        tr.appendChild(td);
      }
      tr.addEventListener('click', () => selectWork(w.key));
      return tr;
    }

    function renderOverview(host) {
      clear(host);
      const header = el('header', { class: 'network-header' });
      header.appendChild(el('h2', null, t(i18n, 'ui.network_title')));
      const toolbar = el('div', { class: 'team-toolbar' });
      const personalBtn = el(
        'button',
        { class: 'btn' + (state.viewMode === 'personal' ? ' primary' : ''), type: 'button' },
        t(i18n, 'ui.personal_view')
      );
      const teamBtn = el(
        'button',
        { class: 'btn' + (state.viewMode === 'team' ? ' primary' : ''), type: 'button' },
        t(i18n, 'ui.team_view')
      );
      if (isLive()) {
        personalBtn.disabled = true;
        personalBtn.title = t(i18n, 'ui.team_personal_unavailable');
      }
      personalBtn.addEventListener('click', () => {
        state.viewMode = 'personal';
        refresh();
      });
      teamBtn.addEventListener('click', () => {
        state.viewMode = 'team';
        refresh();
      });
      const cardsBtn = el(
        'button',
        { class: 'btn' + (state.layout === 'cards' ? ' primary' : ''), type: 'button' },
        t(i18n, 'ui.network_graph_view')
      );
      const listBtn = el(
        'button',
        { class: 'btn' + (state.layout === 'list' ? ' primary' : ''), type: 'button' },
        t(i18n, 'ui.list_layout')
      );
      cardsBtn.setAttribute('aria-pressed', String(state.layout === 'cards'));
      listBtn.setAttribute('aria-pressed', String(state.layout === 'list'));
      cardsBtn.addEventListener('click', () => {
        state.layout = 'cards';
        render();
      });
      listBtn.addEventListener('click', () => {
        state.layout = 'list';
        render();
      });
      if (state.raw && !isLive()) {
        const modes = el('div', { class: 'team-segment' });
        modes.appendChild(personalBtn); modes.appendChild(teamBtn);
        toolbar.appendChild(modes);
      }
      if (state.works.length || state.streams.length) {
        const layouts = el('div', { class: 'team-segment' });
        layouts.appendChild(cardsBtn); layouts.appendChild(listBtn);
        toolbar.appendChild(layouts);
      }
      header.appendChild(toolbar);
      host.appendChild(header);

      if (state.disconnect) {
        const banner = el('div', { class: 'banner' });
        banner.appendChild(el('span', null, t(i18n, 'ui.reconnect_needed')));
        const re = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.reconnect'));
        re.addEventListener('click', () => refresh());
        banner.appendChild(re);
        host.appendChild(banner);
      }

      if (state.raw && !isLive()) host.appendChild(el('p', { class: 'network-demo-note' }, t(i18n, 'ui.network_demo')));
      if (!state.works.length && !state.streams.length) {
        host.appendChild(el('p', { class: 'network-empty', role: 'status' },
          t(i18n, state.loading ? 'ui.team_loading' : state.projectKey ? 'ui.network_empty' : 'ui.network_no_projects')));
        return;
      }
      net = network.model(state.works, state.streams, t(i18n, 'ui.network_unassigned'));
      if (isLive()) {
        const loaded = state.works.filter(w => w.detail_loaded).length;
        const coverage = el('div', { class: 'network-coverage', role: 'status' });
        coverage.appendChild(el('span', null, t(i18n, 'ui.network_coverage', { loaded, total: state.works.length })));
        if (state.graphLoading) coverage.appendChild(el('span', null, t(i18n, 'ui.team_detail_loading')));
        else if (loaded < state.works.length) {
          const load = el('button', { class: 'linkish', type: 'button' }, t(i18n, 'ui.network_load_details'));
          load.addEventListener('click', () => loadGraphDetails());
          coverage.appendChild(load);
        }
        if (state.graphLoading || loaded < state.works.length) host.appendChild(coverage);
      }
      if (state.layout === 'cards') {
        network.render(host, net, { el, card: workCard, count: state.works.length,
          project: state.projects.find(p => p.key === state.projectKey) || { key: state.projectKey },
          text: (key, vars) => t(i18n, 'ui.network_' + key, vars) });
      } else {
        const table = el('table', { class: 'team-table' });
        const thead = el('thead');
        const hr = el('tr');
        for (const h of [
          'key',
          'title',
          'owner',
          'agent',
          'outcome',
          'blocker',
          'next',
        ]) {
          hr.appendChild(el('th', null, t(i18n, 'ui.team_column_' + h)));
        }
        thead.appendChild(hr);
        table.appendChild(thead);
        const tbody = el('tbody');
        for (const w of state.works) tbody.appendChild(workRow(w));
        table.appendChild(tbody);
        const wrapper = el('div', { class: 'team-table-wrap' });
        wrapper.appendChild(table);
        host.appendChild(wrapper);
      }
    }

    function selectedWork() {
      return state.works.find((w) => w.key === state.selected) || null;
    }

    function isLive() { return state.raw && state.raw.interaction_mode === 'mcp'; }

    function workStatus(w) {
      const known = ['planned', 'unclaimed', 'claimed', 'in_progress', 'running', 'blocked', 'waiting', 'in_review', 'review', 'completed', 'accepted', 'cancelled'];
      if (w.status && w.status !== 'unknown') return known.includes(w.status) ? t(i18n, 'ui.network_status_' + w.status) : w.status;
      return t(i18n, w.detail_loaded ? 'ui.team_no_runtime' : 'ui.network_unread');
    }

    async function readWork(work) {
      if (work.detail_loaded) return { ok: true, work };
      if (pendingDetails.has(work.key)) return pendingDetails.get(work.key);
      const current = generation;
      const params = new URLSearchParams({ project: state.projectKey, work: work.key, workstream: work.workstream_id });
      if (work.contract_hash) params.set('contract', work.contract_hash);
      const pending = api('/api/team/work?' + params).then(body => {
        if (current !== generation) return null;
        if (!body || !body.ok || !body.work || body.work.key !== work.key || body.work.workstream_id !== work.workstream_id) {
          failed(body && body.error ? body : null);
          return null;
        }
        Object.assign(work, body.work);
        return body;
      }).finally(() => { if (current === generation) pendingDetails.delete(work.key); });
      pendingDetails.set(work.key, pending);
      return pending;
    }

    async function loadGraphDetails() {
      if (!isLive() || state.graphLoading) return;
      const current = generation;
      const queue = state.works.filter(w => !w.detail_loaded).slice(0, 60);
      state.graphLoading = true;
      render();
      // Bound concurrent reads and total work per batch; large projects stay navigable.
      let index = 0;
      await Promise.all(Array.from({ length: Math.min(4, queue.length) }, async () => {
        while (current === generation && index < queue.length) await readWork(queue[index++]);
      }));
      if (current !== generation) return;
      state.graphLoading = false;
      render();
    }

    async function selectWork(key) {
      const request = ++detailGeneration;
      state.selected = key;
      state.detailResponse = null;
      const work = selectedWork();
      if (!work || !isLive()) { render(); return; }
      const current = generation;
      state.detailLoading = !work.detail_loaded;
      state.error = null;
      render();
      const body = await readWork(work);
      if (current !== generation || request !== detailGeneration || state.selected !== key) return;
      state.detailLoading = false;
      if (body) state.detailResponse = body;
      render();
    }

    function renderBlockerDetail(host, w) {
      host.appendChild(el('h3', null, t(i18n, 'ui.blocker_detail')));
      const b = w.blocker;
      if (!b) {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.no_blocker')));
        return;
      }
      if (typeof b === 'string') {
        host.appendChild(el('p', null, b));
        return;
      }
      host.appendChild(
        el('p', null, t(i18n, 'ui.prerequisite_outcome_p0', { p0: b.prerequisite_outcome || '—' }))
      );
      host.appendChild(
        el('p', null, t(i18n, 'ui.release_condition_p0', { p0: b.release_condition || '—' }))
      );
      host.appendChild(
        el('p', null, t(i18n, 'ui.check_basis_p0', { p0: b.check_basis || '—' }))
      );
      const deps = el('ul', { class: 'loops' });
      for (const d of w.depends_on || []) {
        if (!d.visible) continue;
        const li = el('li');
        const link = el('button', { class: 'linkish', type: 'button' }, d.key);
        link.addEventListener('click', () => {
          state.selected = d.key;
          render();
        });
        li.appendChild(link);
        li.appendChild(
          document.createTextNode(
            ` [${d.status || ''}] ${d.prerequisite_outcome || ''} → ${d.release_condition || ''}`
          )
        );
        deps.appendChild(li);
      }
      host.appendChild(el('h4', null, t(i18n, 'ui.visible_dependencies')));
      host.appendChild(deps);
      for (const h of w.hidden_deps || []) {
        host.appendChild(
          el('p', { class: 'sub' }, t(i18n, 'ui.hidden_dep_hint_p0', { p0: h.hint || '' }))
        );
      }
      const detail = el('details');
      detail.appendChild(el('summary', null, t(i18n, 'ui.backend_detail_layer')));
      detail.appendChild(
        el('pre', { class: 'raw' }, JSON.stringify({
          backend_code: b.backend_code || null,
          raw_receipt_ref: b.raw_receipt_ref || null,
        }, null, 2))
      );
      host.appendChild(detail);
    }

    function guardDouble(action, fn) {
      return async () => {
        if (state.inflight[action]) return;
        state.inflight[action] = true;
        try {
          await fn();
        } finally {
          state.inflight[action] = false;
          render();
        }
      };
    }

    async function runAction(action, extra) {
      const w = selectedWork();
      if (!w) return;
      const current = generation;
      const requestId = `${action}:${w.key}:${Date.now()}`;
      const body = await api('/api/team/action', {
        method: 'POST',
        body: JSON.stringify({
          project: state.projectKey,
          work_key: w.key,
          action,
          request_id: requestId,
          expected_receipt: state.lastReceipts[action + ':' + w.key] || null,
          ...extra,
        }),
      });
      if (current !== generation) return body;
      if (!body || !body.ok) return failed(body);
      if (body && body.ok && body.receipt) {
        state.lastReceipts[action + ':' + w.key] = body.receipt.id || body.receipt.request_id;
        // Exact replay returns the same receipt id.
        if (body.replayed) {
          /* idempotent */
        }
      }
      await refresh();
      return body;
    }

    function mcpUrl() {
      return state.raw && state.raw.mcp_url || '/v1/projects/' + encodeURIComponent(state.projectKey) + '/mcp';
    }
    function continuation(w) {
      return t(i18n, 'ui.team_task_prompt', { url: mcpUrl(), work: w.key, stream: w.workstream_id });
    }
    function copyBlock(host, text, label) {
      host.appendChild(el('pre', { class: 'team-connect-code' }, text));
      const button = el('button', { class: 'btn', type: 'button' }, t(i18n, label));
      const status = el('span', { class: 'sub', role: 'status' });
      button.addEventListener('click', async () => {
        try { await root.navigator.clipboard.writeText(text); status.textContent = t(i18n, 'ui.team_copied'); }
        catch (_) { status.textContent = t(i18n, 'ui.team_copy_manual'); }
      });
      host.appendChild(button); host.appendChild(status);
    }
    function renderConnect(host) {
      const details = el('details', { class: 'team-connect' });
      details.open = state.connectOpen;
      details.addEventListener('toggle', () => { state.connectOpen = details.open; });
      details.appendChild(el('summary', null, t(i18n, 'ui.team_connect_agent')));
      details.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_connect_help')));
      copyBlock(details, 'codex mcp add awr_team --url ' + mcpUrl() + ' --bearer-token-env-var AWR_TEAM_BEARER', 'ui.team_copy_command');
      details.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_connect_other')));
      details.appendChild(el('code', null, mcpUrl()));
      copyBlock(details, t(i18n, 'ui.team_project_prompt', { url: mcpUrl() }), 'ui.team_copy_project_prompt');
      host.appendChild(details);
    }

    function renderActions(host, w) {
      if (isLive()) {
        host.appendChild(el('h3', null, t(i18n, 'ui.team_agent_workflow')));
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_agent_workflow_help')));
        const handoff = el('details', { class: 'team-connect' });
        const handoffKey = JSON.stringify([state.projectKey, w.key]);
        handoff.open = Boolean(state.handoffOpen[handoffKey]);
        handoff.addEventListener('toggle', () => { state.handoffOpen[handoffKey] = handoff.open; });
        handoff.appendChild(el('summary', null, t(i18n, 'ui.team_agent_handoff')));
        copyBlock(handoff, continuation(w), 'ui.team_copy_handoff');
        host.appendChild(handoff);
        return;
      }
      host.appendChild(el('h3', null, t(i18n, 'ui.collaboration_actions')));
      const caps = w.capabilities || {};
      host.appendChild(
        el(
          'p',
          { class: 'sub' },
          t(i18n, 'ui.run_pause_visibility_p0', {
            p0: `run=${Boolean(caps.run)} pause=${Boolean(caps.pause)}`,
          })
        )
      );
      const actions = [
        ['accept_responsibility', 'ui.accept_responsibility'],
        ['select_agent', 'ui.select_authorized_agent'],
        ['respond_blocker', 'ui.respond_to_blocker'],
        ['handoff_receive', 'ui.receive_handoff'],
        ['submit_review', 'ui.submit_review'],
        ['rework', 'ui.rework'],
        ['accept', 'ui.accept_work'],
      ];
      const bar = el('div', { class: 'team-actions' });
      for (const [action, label] of actions) {
        const btn = el('button', { class: 'btn', type: 'button' }, t(i18n, label));
        const payload =
          action === 'select_agent'
            ? { agent_id: (state.members[0] && state.members[0].agents[0]) || 'coding-agent' }
            : {};
        btn.addEventListener(
          'click',
          guardDouble(action + ':' + w.key, () => runAction(action, payload))
        );
        // Second listener proves double-click is ignored while inflight.
        btn.addEventListener('dblclick', (e) => e.preventDefault());
        bar.appendChild(btn);
      }
      host.appendChild(bar);
      const receipt = state.lastReceipts;
      host.appendChild(
        el('pre', { class: 'raw' }, JSON.stringify(receipt, null, 2))
      );
    }

    function renderDetail(host) {
      clear(host);
      const w = selectedWork();
      if (!w) {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.select_a_card_or_row')));
        return;
      }
      const header = el('div', { class: 'task-detail-heading' });
      const lane = state.streams.find(s => s.id === w.workstream_id);
      header.appendChild(el('span', null, [w.key, lane && (lane.title || lane.external_key)].filter(Boolean).join(' · ')));
      header.appendChild(el('h4', null, w.title || w.key));
      header.appendChild(el('em', { class: 'detail-state', dataset: { status: network.visualStatus(w) } }, workStatus(w)));
      if (w.description) header.appendChild(el('p', null, w.description));
      host.appendChild(header);
      renderActions(host, w);
      const unknown = t(i18n, 'ui.network_not_reported');
      const section = (title, rows) => {
        const group = el('section', { class: 'detail-section' });
        group.appendChild(el('h5', null, t(i18n, 'ui.network_' + title)));
        const list = el('dl');
        for (const [key, value] of rows) {
          const row = el('div'); row.appendChild(el('dt', null, t(i18n, 'ui.network_' + key)));
          row.appendChild(el('dd', null, value == null || value === '' ? unknown : value)); list.appendChild(row);
        }
        group.appendChild(list); host.appendChild(group);
      };
      const agent = w.agent && typeof w.agent === 'object' ? w.agent.id : w.agent;
      section('people', [['developer', w.owner_person], ['agent', agent], ['model', w.model], ['session', w.session_id], ['tokens', null]]);
      section('progress', [['status', workStatus(w)], ['next', w.next_step]]);
      section('related', [['pr', null], ['ci', null]]);
      if (isLive()) {
        if (state.detailLoading) {
          host.appendChild(el('p', { role: 'status' }, t(i18n, 'ui.team_detail_loading')));
          return;
        }
        if (!w.detail_loaded) {
          host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_detail_unavailable')));
          return;
        }

        host.appendChild(el('h3', null, t(i18n, 'ui.acceptance_criteria')));
        const acceptance = el('ul', { class: 'loops' });
        for (const criterion of w.acceptance || []) acceptance.appendChild(el('li', null, criterion));
        host.appendChild(acceptance);
        host.appendChild(el('h3', null, t(i18n, 'ui.visible_dependencies')));
        const dependencies = el('ul', { class: 'loops' });
        for (const dependency of w.depends_on || []) {
          if (dependency.visible !== true) continue;
          const item = el('li');
          const target = state.works.find((other) => other.key === dependency.key);
          const link = el(target ? 'button' : 'span', target ? { class: 'linkish', type: 'button' } : null, dependency.key);
          if (target) link.addEventListener('click', () => selectWork(dependency.key));
          item.appendChild(link);
          dependencies.appendChild(item);
        }
        host.appendChild(dependencies);
        if (w.dependency_export_unavailable) host.appendChild(el('p', { class: 'team-error' }, t(i18n, 'ui.team_dependency_unavailable')));
        if (w.recovery_blocked) host.appendChild(el('p', { class: 'team-error' }, 'RecoveryBlocked'));
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_admission_not_evaluated')));
        if (!w.context_complete) host.appendChild(el('p', { class: 'sub' }, (w.completeness_reasons || []).join(' · ')));
        return;
      }
      renderBlockerDetail(host, w);
      host.appendChild(el('h3', null, t(i18n, 'ui.dependency_graph')));
      const graph = el('ul', { class: 'loops' });
      for (const d of w.depends_on || []) {
        if (!d.visible) continue;
        graph.appendChild(
          el('li', null, `${d.key} → ${w.key} · ${d.prerequisite_outcome || ''}`)
        );
      }
      host.appendChild(graph);
    }

    function renderAuth(host) {
      const refocusInput = loginInput && document.activeElement === loginInput;
      clear(host);
      if (state.session && state.session.session_id) {
        if (loginInput) loginInput.value = '';
        loginForm = null;
        loginInput = null;
        host.appendChild(
          el('p', null, t(i18n, 'ui.team_signed_in'))
        );
        const logout = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.logout'));
        logout.addEventListener(
          'click', guardDouble('logout', () => signOut('/api/team/logout', {}))
        );
        const revoke = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.revoke_session'));
        revoke.addEventListener(
          'click',
          guardDouble('revoke', () => signOut('/api/team/session/revoke', { all_mine: true }))
        );
        host.appendChild(logout);
        host.appendChild(revoke);
        if (isLive() && state.projectKey) renderConnect(host);
      } else {
        host.appendChild(el('label', { class: 'team-login-label', for: 'teamBearerInput' }, t(i18n, 'ui.team_access_token')));
        // A pending anonymous refresh must not discard a credential being typed.
        // Keep the form nodes, never copy the credential into application state.
        if (!loginForm) {
          const form = el('div', { class: 'team-login' });
          const input = el('input', {
            type: 'password',
            id: 'teamBearerInput',
            autocomplete: 'off',
            'aria-label': t(i18n, 'ui.team_access_token'),
            placeholder: 'awr1.…',
          });
          const btn = el('button', { class: 'btn primary', type: 'button' }, t(i18n, 'ui.web_login'));
          btn.addEventListener(
            'click',
            guardDouble('login', async () => {
              const current = ++generation;
              clearProjectData();
              state.error = null;
              const bearer = input.value;
              input.value = ''; // never retain bearer in the DOM after submit
              const body = await api('/api/team/login', {
                method: 'POST',
                body: JSON.stringify({ bearer }),
              });
              if (current !== generation) return;
              if (body && body.ok) {
                state.session = {
                  session_id: body.session_id,
                  expires_at_ms: body.expires_at_ms,
                };
                await refresh();
              } else failed(body);
            })
          );
          form.appendChild(input);
          form.appendChild(btn);
          input.addEventListener('keydown', (event) => {
            if (event.key === 'Enter') { event.preventDefault(); btn.click(); }
          });
          loginForm = form;
          loginInput = input;
        }
        host.appendChild(loginForm);
        host.appendChild(el('p', { class: 'team-login-help' }, t(i18n, 'ui.web_login_help')));
        if (refocusInput) loginInput.focus();
      }
      if (state.error) {
        const message = el('p', { class: 'team-error', role: 'alert' });
        message.appendChild(el('strong', null, state.error.code));
        message.appendChild(document.createTextNode(' · ' + state.error.message));
        host.appendChild(message);
        if (state.disconnect) host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_unknown_outcome')));
      }
    }

    async function signOut(path, payload) {
      const current = ++generation;
      clearProjectData();
      state.error = null;
      state.loading = false;
      render();
      const body = await api(path, { method: 'POST', body: JSON.stringify(payload) });
      if (current !== generation) return;
      if (body && body.ok) state.session = null;
      else failed(body);
    }

    function clear(node) {
      while (node && node.firstChild) node.removeChild(node.firstChild);
    }

    function render() {
      const projects = $('teamProjects');
      const overview = $('teamOverview');
      const detail = $('teamDetail');
      const auth = $('teamAuth');
      const signedIn = Boolean(state.session && state.session.session_id);
      const view = $('view-team');
      if (view) view.dataset.teamState = signedIn ? 'signed-in' : 'signed-out';
      const grid = $('teamWorkspaceGrid');
      if (grid) grid.hidden = !signedIn;
      const intro = $('teamWorkspaceIntro');
      if (intro) intro.hidden = signedIn;
      const rawSection = $('teamRaw');
      if (rawSection) rawSection.hidden = !signedIn || !state.raw;
      if (projects) projects.hidden = !signedIn || !state.projects.length;
      if (detail) detail.hidden = !signedIn || !state.works.length;
      const active = typeof document !== 'undefined' ? document.activeElement : null;
      const activeKey = active && active.dataset && active.dataset.key;
      const frame = overview && overview.querySelector('.scene-frame');
      const scroll = frame ? [frame.scrollLeft, frame.scrollTop] : [0, 0];
      if (auth) { auth.classList.toggle('signed-in', signedIn); renderAuth(auth); }
      if (projects) renderProjects(projects);
      if (overview) renderOverview(overview);
      if (detail) renderDetail(detail);
      if (overview && net) {
        network.layout(overview, net);
        const nextFrame = overview.querySelector('.scene-frame');
        if (nextFrame) { nextFrame.scrollLeft = scroll[0]; nextFrame.scrollTop = scroll[1]; }
        if (activeKey) {
          const card = [...overview.querySelectorAll('.task-node')].find(n => n.dataset.key === activeKey);
          if (card) card.focus({ preventScroll: true });
        }
      }
      const raw = $('rawTeamBody');
      if (raw) {
        // The upstream session identifier is a cookie credential, not debug data.
        const { session, ...overviewData } = state.raw || {};
        raw.textContent = state.raw ? JSON.stringify({ overview: overviewData, detail: state.detailResponse }, null, 2) : '';
      }
    }

    async function refresh() {
      const current = ++generation;
      state.error = null;
      state.loading = true;
      state.works = [];
      state.streams = [];
      state.graphLoading = false;
      pendingDetails = new Map();
      state.members = [];
      state.raw = null;
      state.detailLoading = false;
      state.detailResponse = null;
      render();
      const projects = await api('/api/team/projects?view=' + encodeURIComponent(state.viewMode));
      if (current !== generation) return;
      if (!projects || !projects.ok || !Array.isArray(projects.projects)) {
        clearProjectData();
        if (projects && projects.error && projects.error.code === 'Unauthenticated' && !state.session) {
          state.error = null; // The first visit is a normal sign-in state.
        } else failed(projects);
      } else {
        state.projects = projects.projects;
        state.session = projects.session || state.session;
        if (!state.projects.some((p) => p.key === state.projectKey)) {
          state.projectKey = state.projects[0] ? state.projects[0].key : null;
          state.selected = null;
          state.lastReceipts = Object.create(null);
        }
        if (state.projectKey) {
          const overview = await api('/api/team/overview?project=' + encodeURIComponent(state.projectKey) + '&view=' + encodeURIComponent(state.viewMode));
          if (current !== generation) return;
          if (!overview || !overview.ok || !Array.isArray(overview.works)) failed(overview);
          else {
            state.works = overview.works.map((work) => ({ ...work }));
            state.members = overview.members || [];
            state.streams = overview.workstreams || [];
            state.session = overview.session || state.session;
            state.raw = overview;
            state.disconnect = false;
          }
        }
      }
      if (!selectedWork()) state.selected = null;
      state.loading = false;
      render();
      if (isLive()) {
        if (state.streams.length) {
          await loadGraphDetails();
          if (current !== generation) return;
          if (!state.selected && state.works.length) state.selected = state.works[0].key;
        }
        if (state.selected) await selectWork(state.selected);
      }
    }

    const overviewHost = $('teamOverview');
    if (overviewHost && typeof ResizeObserver !== 'undefined') {
      new ResizeObserver(() => { if (net) network.layout(overviewHost, net); }).observe(overviewHost);
    }

    return {
      state,
      refresh,
      render,
      api,
      // test hooks
      _guardDouble: guardDouble,
      _runAction: runAction,
      _selectedWork: selectedWork,
      _selectWork: selectWork,
      _loadGraphDetails: loadGraphDetails,
    };
  }

  const api = { createTeamWeb };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_WEB = api;
})(typeof window !== 'undefined' ? window : globalThis);
