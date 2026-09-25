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
      error: null,
      loading: false,
      detailLoading: false,
      detailResponse: null,
    };
    let generation = 0;
    let detailGeneration = 0;
    let loginForm = null;
    let loginInput = null;

    function clearProjectData() {
      state.projects = [];
      state.projectKey = null;
      state.works = [];
      state.members = [];
      state.selected = null;
      state.raw = null;
      state.detailLoading = false;
      state.detailResponse = null;
      state.lastReceipts = Object.create(null);
    }

    function failed(body) {
      state.error = (body && body.error) || { code: 'InvalidResponse', message: 'Invalid Team response' };
      state.disconnect = state.error.code === 'BridgeUnreachable';
      if (['Unauthenticated', 'SessionExpired'].includes(state.error.code)) {
        state.session = null;
        clearProjectData();
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
      const ul = el('ul', { class: 'team-project-list' });
      for (const p of state.projects) {
        const li = el('li');
        const btn = el('button', {
          class: 'btn' + (p.key === state.projectKey ? ' primary' : ''),
          type: 'button',
        }, [p.title || p.key, p.role].filter(Boolean).join(' · '));
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
        tabindex: '0', role: 'button', 'aria-label': w.title || w.key,
      });
      card.appendChild(el('h3', null, w.title || w.key));
      card.appendChild(el('div', { class: 'sub' }, w.key + ' · ' + workStatus(w)));
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
      if (w.status && w.status !== 'unknown') return w.status;
      return t(i18n, w.detail_loaded ? 'ui.team_no_runtime' : 'ui.team_select_for_status');
    }

    async function selectWork(key) {
      const request = ++detailGeneration;
      state.selected = key;
      state.detailResponse = null;
      const work = selectedWork();
      if (!work || !isLive()) { render(); return; }
      const current = generation;
      state.detailLoading = true;
      state.error = null;
      render();
      const params = new URLSearchParams({ project: state.projectKey, work: key, workstream: work.workstream_id });
      if (work.contract_hash) params.set('contract', work.contract_hash);
      const body = await api('/api/team/work?' + params);
      if (current !== generation || request !== detailGeneration || state.selected !== key) return;
      state.detailLoading = false;
      if (!body || !body.ok || !body.work || body.work.key !== key) failed(body);
      else {
        Object.assign(work, body.work);
        state.detailResponse = body;
      }
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

    function renderActions(host, w) {
      host.appendChild(el('h3', null, t(i18n, 'ui.collaboration_actions')));
      if (isLive()) {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_mcp_actions')));
        host.appendChild(el('code', null, '/v1/projects/' + encodeURIComponent(state.projectKey) + '/mcp'));
        return;
      }
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
      if (isLive()) {
        if (state.detailLoading) {
          host.appendChild(el('p', { role: 'status' }, t(i18n, 'ui.team_detail_loading')));
          return;
        }
        if (!w.detail_loaded) {
          host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.team_detail_unavailable')));
          return;
        }
        host.appendChild(el('p', null, workStatus(w)));
        host.appendChild(el('h3', null, t(i18n, 'ui.acceptance_criteria')));
        const acceptance = el('ul', { class: 'loops' });
        for (const criterion of w.acceptance) acceptance.appendChild(el('li', null, criterion));
        host.appendChild(acceptance);
        host.appendChild(el('h3', null, t(i18n, 'ui.visible_dependencies')));
        const dependencies = el('ul', { class: 'loops' });
        for (const dependency of w.depends_on) {
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
        if (!w.context_complete) host.appendChild(el('p', { class: 'sub' }, w.completeness_reasons.join(' · ')));
        renderActions(host, w);
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
      renderActions(host, w);
    }

    function renderAuth(host) {
      const refocusInput = loginInput && document.activeElement === loginInput;
      clear(host);
      host.appendChild(el('h3', null, t(i18n, 'ui.web_session')));
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
      } else {
        host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.web_login_help')));
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
        if (refocusInput) loginInput.focus();
      }
      host.appendChild(el('p', { class: 'sub' }, t(i18n, 'ui.members_roles_via_access')));
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
      if (auth) renderAuth(auth);
      if (projects) renderProjects(projects);
      if (overview) renderOverview(overview);
      if (detail) renderDetail(detail);
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
            state.session = overview.session || state.session;
            state.raw = overview;
            state.disconnect = false;
          }
        }
      }
      if (!selectedWork()) state.selected = null;
      state.loading = false;
      render();
      if (state.selected && isLive()) await selectWork(state.selected);
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
