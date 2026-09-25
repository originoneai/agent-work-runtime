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
    const callApi = opts.callApi;
    const state = {
      viewMode: 'team', // personal | team
      layout: 'cards', // cards | list
      projectKey: null,
      projects: [],
      works: [],
      selected: null,
      session: null,
      disconnect: false,
      inflight: Object.create(null),
      lastReceipts: Object.create(null),
      members: [],
      raw: null,
    };

    async function api(path, options) {
      const opts2 = Object.assign({ credentials: 'same-origin' }, options || {});
      opts2.headers = Object.assign(
        { 'content-type': 'application/json', [GUARD]: '1' },
        opts2.headers || {}
      );
      try {
        const res = await fetch(path, opts2);
        const body = await res.json();
        if (body && body.error && body.error.code === 'SessionExpired') {
          state.session = null;
          state.disconnect = true;
        }
        return body;
      } catch (err) {
        state.disconnect = true;
        return {
          ok: false,
          error: { code: 'BridgeUnreachable', message: String(err && err.message) },
        };
      }
    }

    async function loadProjects() {
      const body = await api('/api/team/projects?view=' + encodeURIComponent(state.viewMode));
      if (!body || !body.ok) return body;
      state.projects = body.projects || [];
      state.session = body.session || state.session;
      state.disconnect = false;
      if (!state.projectKey && state.projects[0]) state.projectKey = state.projects[0].key;
      return body;
    }

    async function loadOverview() {
      if (!state.projectKey) return { ok: false };
      const q =
        '/api/team/overview?project=' +
        encodeURIComponent(state.projectKey) +
        '&view=' +
        encodeURIComponent(state.viewMode);
      const body = await api(q);
      if (!body || !body.ok) return body;
      state.works = body.works || [];
      state.members = body.members || [];
      state.raw = body;
      state.disconnect = false;
      return body;
    }

    function renderProjects(host) {
      clear(host);
      host.appendChild(el('h3', null, t(i18n, 'ui.my_projects')));
      if (!state.projects.length) {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.no_projects_yet')));
        return;
      }
      const ul = el('ul', { class: 'team-project-list' });
      for (const p of state.projects) {
        const li = el('li');
        const btn = el('button', {
          class: 'btn' + (p.key === state.projectKey ? ' primary' : ''),
          type: 'button',
        }, `${p.title || p.key} · ${p.role || ''}`);
        btn.addEventListener('click', () => {
          state.projectKey = p.key;
          refresh();
        });
        li.appendChild(btn);
        ul.appendChild(li);
      }
      host.appendChild(ul);
    }

    function workCard(w) {
      const card = el('article', {
        class: 'team-card' + (state.selected === w.key ? ' selected' : ''),
        dataset: { key: w.key },
      });
      card.appendChild(el('h3', null, w.title || w.key));
      card.appendChild(el('div', { class: 'sub' }, w.key + ' · ' + (w.status || '')));
      card.appendChild(
        el('div', null, t(i18n, 'ui.owner_p0', { p0: w.owner_person || '—' }))
      );
      const agent =
        w.agent && typeof w.agent === 'object'
          ? `${w.agent.id || ''} (${w.agent.state || ''})`
          : w.agent || '—';
      card.appendChild(el('div', null, t(i18n, 'ui.agent_running_p0', { p0: agent })));
      card.appendChild(
        el('div', null, t(i18n, 'ui.outcome_p0', { p0: w.outcome || '—' }))
      );
      const blocker =
        w.blocker && typeof w.blocker === 'object' ? w.blocker.summary : w.blocker;
      card.appendChild(
        el('div', null, t(i18n, 'ui.blocker_reason_p0', { p0: blocker || '—' }))
      );
      card.appendChild(
        el('div', null, t(i18n, 'ui.next_step_p0', { p0: w.next_step || '—' }))
      );
      card.addEventListener('click', () => {
        state.selected = w.key;
        render();
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
        tr.appendChild(el('td', null, text || '—'));
      }
      tr.addEventListener('click', () => {
        state.selected = w.key;
        render();
      });
      return tr;
    }

    function renderOverview(host) {
      clear(host);
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
        t(i18n, 'ui.card_layout')
      );
      const listBtn = el(
        'button',
        { class: 'btn' + (state.layout === 'list' ? ' primary' : ''), type: 'button' },
        t(i18n, 'ui.list_layout')
      );
      cardsBtn.addEventListener('click', () => {
        state.layout = 'cards';
        render();
      });
      listBtn.addEventListener('click', () => {
        state.layout = 'list';
        render();
      });
      toolbar.appendChild(personalBtn);
      toolbar.appendChild(teamBtn);
      toolbar.appendChild(cardsBtn);
      toolbar.appendChild(listBtn);
      host.appendChild(toolbar);

      if (state.disconnect) {
        const banner = el('div', { class: 'banner' });
        banner.appendChild(el('span', null, t(i18n, 'ui.reconnect_needed')));
        const re = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.reconnect'));
        re.addEventListener('click', () => refresh());
        banner.appendChild(re);
        host.appendChild(banner);
      }

      host.appendChild(el('h3', null, t(i18n, 'ui.parallel_overview')));
      if (state.layout === 'cards') {
        const grid = el('div', { class: 'team-card-grid' });
        for (const w of state.works) grid.appendChild(workCard(w));
        host.appendChild(grid);
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
          hr.appendChild(el('th', null, h));
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
      if (body && body.error && body.error.code === 'ExpiredOperation') {
        alert(t(i18n, 'ui.operation_expired'));
        return body;
      }
      if (body && body.ok && body.receipt) {
        state.lastReceipts[action + ':' + w.key] = body.receipt.id || body.receipt.request_id;
        // Exact replay returns the same receipt id.
        if (body.replayed) {
          /* idempotent */
        }
      }
      await loadOverview();
      return body;
    }

    function renderActions(host, w) {
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
      host.appendChild(el('h2', null, w.title || w.key));
      host.appendChild(el('p', { class: 'sub' }, w.key));
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
      renderActions(host, w);
    }

    function renderAuth(host) {
      clear(host);
      host.appendChild(el('h3', null, t(i18n, 'ui.web_session')));
      if (state.session && state.session.session_id) {
        host.appendChild(
          el('p', null, t(i18n, 'ui.signed_in_session_p0', { p0: state.session.session_id }))
        );
        const logout = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.logout'));
        logout.addEventListener(
          'click',
          guardDouble('logout', async () => {
            await api('/api/team/logout', { method: 'POST', body: '{}' });
            state.session = null;
          })
        );
        const revoke = el('button', { class: 'btn', type: 'button' }, t(i18n, 'ui.revoke_session'));
        revoke.addEventListener(
          'click',
          guardDouble('revoke', async () => {
            await api('/api/team/session/revoke', {
              method: 'POST',
              body: JSON.stringify({ all_mine: true }),
            });
            state.session = null;
          })
        );
        host.appendChild(logout);
        host.appendChild(revoke);
      } else {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.web_login_help')));
        const form = el('div', { class: 'team-login' });
        const input = el('input', {
          type: 'password',
          id: 'teamBearerInput',
          autocomplete: 'off',
          placeholder: 'awr1.…',
        });
        const btn = el('button', { class: 'btn primary', type: 'button' }, t(i18n, 'ui.web_login'));
        btn.addEventListener(
          'click',
          guardDouble('login', async () => {
            const bearer = input.value;
            input.value = ''; // never retain bearer in the DOM after submit
            const body = await api('/api/team/login', {
              method: 'POST',
              body: JSON.stringify({ bearer }),
            });
            if (body && body.ok) {
              state.session = {
                session_id: body.session_id,
                expires_at_ms: body.expires_at_ms,
              };
            }
          })
        );
        form.appendChild(input);
        form.appendChild(btn);
        host.appendChild(form);
      }
      host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.members_roles_via_access')));
    }

    function clear(node) {
      while (node && node.firstChild) node.removeChild(node.firstChild);
    }

    function render() {
      const projects = $('teamProjects');
      const overview = $('teamOverview');
      const detail = $('teamDetail');
      const auth = $('teamAuth');
      if (auth) renderAuth(auth);
      if (projects) renderProjects(projects);
      if (overview) renderOverview(overview);
      if (detail) renderDetail(detail);
    }

    async function refresh() {
      await loadProjects();
      await loadOverview();
      render();
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
    };
  }

  const api = { createTeamWeb };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_WEB = api;
})(typeof window !== 'undefined' ? window : globalThis);
