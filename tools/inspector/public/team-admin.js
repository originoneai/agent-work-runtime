/** Project member administration and scoped audit views. Secrets stay in closure memory. */
(function (root) {
  'use strict';
  function element(tag, text, className) {
    const n = document.createElement(tag);
    if (text != null) n.textContent = text;
    if (className) n.className = className;
    return n;
  }
  function createTeamAdmin({ $, i18n, api, onAuthError }) {
    const onboarding = root.AWR_TEAM_ONBOARDING || (typeof require === 'function' ? require('./team-onboarding') : null);
    const t = (key, vars) => i18n.t('admin.' + key, vars);
    let context = null, generation = 0, tab = 'graph', busy = false, error = null;
    let members = [], streams = [], nextMember = null, loaded = false;
    let editor = null, pending = null, notice = null;
    let activity = [], nextActivity = null, auditKind = 'development', memberFilter = '', workFilter = '';
    let activityLoaded = false;
    const value = x => x == null ? '' : String(x);
    function button(label, action, disabled = false) {
      const b = element('button', t(label), 'btn'); b.type = 'button'; b.disabled = disabled;
      b.addEventListener('click', async () => { if (!b.disabled) await action(); }); return b;
    }
    function fail(body) {
      error = body && body.error || { code: 'InvalidResponse', message: 'Invalid response' };
      if (['Unauthenticated', 'SessionExpired'].includes(error.code)) onAuthError(body);
    }
    function reset() {
      ++generation; context = null; tab = 'graph'; busy = false; error = null;
      members = []; streams = []; nextMember = null; loaded = false; editor = null; pending = null;
      activity = []; nextActivity = null; activityLoaded = false; notice = null;
      memberFilter = ''; workFilter = '';
      render();
    }
    function setContext(next) {
      if (!next || !next.identity) { if (context) reset(); return; }
      const same = context && context.project === next.project && context.session === next.session;
      if (!same) reset();
      context = next;
      if (!context.identity.can_manage_members) {
        if (tab === 'members') tab = 'graph'; members = []; editor = null; pending = null;
      }
      render();
    }
    async function access(operation, payload) {
      return api('/api/team/access', { method: 'POST', body: JSON.stringify({
        project: context.project, operation, payload: { protocol_version: 1, ...payload },
      }) });
    }
    async function loadMembers(more = false) {
      if (!context || busy) return;
      const g = generation; busy = true; error = null; render();
      const body = await access('inspect', { limit: 25, ...(more && nextMember ? { cursor: nextMember } : {}) });
      if (g !== generation) return;
      busy = false;
      if (!body.ok || !Array.isArray(body.data && body.data.items)) fail(body);
      else {
        members = more ? members.concat(body.data.items) : body.data.items;
        streams = body.data.workstreams || []; nextMember = body.data.next_cursor; loaded = true;
      }
      render();
    }
    async function loadActivity(more = false) {
      if (!context || busy) return;
      const g = generation; busy = true; error = null; render();
      const params = new URLSearchParams({ project: context.project, kind: auditKind });
      if (memberFilter) params.set('member_actor_id', memberFilter);
      if (workFilter) params.set('work_id', workFilter);
      if (more && nextActivity) params.set('cursor', nextActivity);
      const body = await api('/api/team/activity?' + params);
      if (g !== generation) return;
      busy = false;
      if (!body.ok || !Array.isArray(body.data && body.data.items)) fail(body);
      else { activity = more ? activity.concat(body.data.items) : body.data.items; nextActivity = body.data.next_cursor; activityLoaded = true; }
      render();
    }
    function planFor(member, binding) {
      return { protocol_version: 1, subject: { id: member.actor_id, kind: member.kind, display_name: member.display_name },
        subject_client_id: binding.client_id, role: member.role, independent_review: member.independent_review,
        grants: (binding.grants || []).filter(g => g.active).map(g => ({
          workstream_id: g.workstream_id, authority_version: g.authority_version,
          read: g.read, write: g.write, manage: g.manage,
        })), remove_membership: false, revoke_tenant_credentials: [] };
    }
    async function preview(plan, issue) {
      if (busy || pending) return;
      const g = generation; busy = true; error = null; notice = null;
      render();
      let generated;
      try {
        if (issue) generated = await onboarding.generateCredential(root.crypto);
        if (g !== generation) return;
        if (generated) { plan.credential = generated.credential; plan.credential_project_scoped = true; }
        const body = await access('preview', { plan });
        if (g !== generation) return;
        if (!body.ok) fail(body);
        else {
          pending = { plan, preview: body.data, bearer: generated && generated.bearer,
            requestId: 'access-' + root.crypto.randomUUID(), status: 'preview' };
          editor = null;
        }
      } catch (e) { if (g === generation) error = { code: 'CredentialPreparationFailed', message: String(e.message) }; }
      if (g !== generation) return;
      busy = false; render();
    }
    async function apply() {
      if (busy || !pending || !['preview', 'retry'].includes(pending.status)) return;
      const g = generation, p = pending; busy = true; error = null; p.status = 'sending'; render();
      const body = await access('apply', { plan: p.plan, request_id: p.requestId,
        expected_state: p.preview.state_digest, expected_plan: p.preview.plan_digest });
      if (g !== generation) return;
      busy = false;
      if (body.ok) finish(p);
      else { p.status = ['Forbidden', 'InvalidInput', 'PreconditionsChanged', 'IdempotencyConflict'].includes(body.error && body.error.code) ? 'rejected' : 'unknown'; fail(body); }
      render();
    }
    function finish(p) {
      p.status = 'committed'; notice = t('committed'); loaded = false;
      if (!p.bearer) pending = null;
    }
    async function inspectOutcome() {
      if (busy || !pending) return;
      const g = generation, p = pending; busy = true; error = null; render();
      const body = await access('outcome', { request_id: p.requestId });
      if (g !== generation) return;
      busy = false;
      if (!body.ok) fail(body);
      else if (body.data.outcome === 'committed') finish(p);
      else p.status = 'retry';
      render();
    }
    function field(host, label, input) {
      const l = element('label', null, 'admin-field'); l.appendChild(element('span', t(label))); l.appendChild(input); host.appendChild(l); return input;
    }
    function renderEditor(host) {
      const form = element('form', null, 'admin-editor');
      form.addEventListener('submit', e => e.preventDefault());
      const name = element('input'); name.value = editor.name; name.required = true; name.maxLength = 200; name.disabled = !!editor.member;
      name.addEventListener('input', () => { editor.name = name.value; }); field(form, 'name', name);
      const id = element('input'); id.value = editor.id; id.required = true; id.pattern = '[A-Za-z0-9_-]+'; id.maxLength = 100; id.disabled = !!editor.member;
      id.addEventListener('input', () => { editor.id = id.value; }); field(form, 'actor', id);
      const role = element('select');
      for (const key of ['reader', 'developer', 'reviewer', 'maintainer', 'project_admin']) { const o = element('option', t(key)); o.value = key; role.appendChild(o); }
      role.value = editor.role; role.addEventListener('change', () => { editor.role = role.value; }); field(form, 'role', role);
      const scopes = element('fieldset'); scopes.appendChild(element('legend', t('streams')));
      for (const stream of streams) {
        const l = element('label', null, 'admin-check'); const input = element('input'); input.type = 'checkbox'; input.checked = editor.scopes.includes(stream.id);
        input.addEventListener('change', () => { editor.scopes = input.checked ? [...editor.scopes, stream.id] : editor.scopes.filter(id => id !== stream.id); });
        l.appendChild(input); l.appendChild(element('span', stream.external_key || stream.id)); scopes.appendChild(l);
      }
      form.appendChild(scopes);
      const review = element('input'); review.type = 'checkbox'; review.checked = editor.review;
      review.addEventListener('change', () => { editor.review = review.checked; }); field(form, 'review', review);
      form.appendChild(button('preview', async () => {
        if (!editor.name.trim() || !/^[A-Za-z0-9_-]{1,100}$/.test(editor.id) || !editor.scopes.length) { form.reportValidity && form.reportValidity(); return; }
        const plan = { protocol_version: 1, subject: { id: editor.id, display_name: editor.name.trim(), kind: editor.member ? editor.member.kind : 'human' },
          subject_client_id: editor.binding ? editor.binding.client_id : editor.id + '-agent', role: editor.role,
          independent_review: editor.review, grants: streams.filter(s => editor.scopes.includes(s.id)).map(s => ({
            workstream_id: s.id, authority_version: s.authority_version, read: true,
            write: editor.role !== 'reader' && s.write, manage: editor.role === 'project_admin',
          })), remove_membership: false, revoke_tenant_credentials: [] };
        await preview(plan, !editor.member);
      }, busy));
      form.appendChild(button('cancel', () => { editor = null; render(); }, busy)); host.appendChild(form);
    }
    function edit(member, binding) {
      if (pending || busy) return;
      const aliases = { admin: 'project_admin', worker: 'developer' };
      editor = { member, binding, name: member ? member.display_name : '', id: member ? member.actor_id : '',
        role: member ? aliases[member.role] || member.role : 'developer', review: member ? member.independent_review : false,
        scopes: binding ? (binding.grants || []).filter(g => g.active).map(g => g.workstream_id) : streams.map(s => s.id) };
      error = null; render();
    }
    function renderPending(host) {
      if (!pending) return;
      const box = element('section', null, 'admin-change');
      if (pending.status === 'committed' && pending.bearer) {
        box.appendChild(element('h3', t('secret_title'))); box.appendChild(element('p', t('secret_note'), 'sub'));
        const text = onboarding.instruction(i18n, context.project, context.mcpUrl, pending.bearer);
        const area = element('textarea'); area.readOnly = true; area.value = text; area.rows = 10; area.setAttribute('aria-label', t('secret_title')); box.appendChild(area);
        box.appendChild(button('copy', async () => {
          try { await root.navigator.clipboard.writeText(text); notice = t('copied'); render(); }
          catch (_) { area.focus(); area.select(); }
        }));
        box.appendChild(button('close', async () => { pending = null; render(); await loadMembers(); }));
      } else {
        box.appendChild(element('h3', t('preview')));
        const plan = pending.plan;
        box.appendChild(element('p', [plan.subject.display_name, plan.subject.id, plan.role].join(' · ')));
        box.appendChild(element('p', t('preview_note'), 'sub'));
        const d = element('details'); d.appendChild(element('summary', t('preview')));
        d.appendChild(element('pre', JSON.stringify(pending.preview.desired, null, 2))); box.appendChild(d);
        box.appendChild(element('code', pending.requestId));
        if (['unknown', 'retry'].includes(pending.status)) {
          box.appendChild(element('p', t('unknown'), 'admin-warning')); box.appendChild(button('inspect', inspectOutcome, busy));
          if (pending.status === 'retry') box.appendChild(button('retry', apply, busy));
        } else if (pending.status !== 'rejected') box.appendChild(button('apply', apply, busy));
        if (['preview', 'rejected'].includes(pending.status)) box.appendChild(button('cancel', () => { pending = null; render(); }, busy));
      }
      host.appendChild(box);
    }
    function renderMembers(host) {
      host.appendChild(element('h2', t('members'))); host.appendChild(element('p', t('member_note'), 'sub'));
      const toolbar = element('div', null, 'admin-toolbar');
      toolbar.appendChild(button('add', () => edit(null, null), busy || !!pending || !loaded));
      toolbar.appendChild(button('refresh', () => loadMembers(), busy || !!pending)); host.appendChild(toolbar);
      if (editor) renderEditor(host);
      renderPending(host);
      host.appendChild(element('p', t('scope_note'), 'sub'));
      const list = element('div', null, 'admin-members');
      for (const member of members) {
        const row = element('article', null, 'admin-member');
        row.appendChild(element('h3', member.display_name)); row.appendChild(element('p', member.actor_id + ' · ' + t(({admin:'project_admin', worker:'developer'})[member.role] || member.role), 'sub'));
        const bindings = member.clients.length ? member.clients : [{ client_id: member.actor_id + '-agent', grants: [], credentials: [] }];
        for (const binding of bindings) {
          const controls = element('div', null, 'admin-binding'); controls.appendChild(element('code', binding.client_id));
          controls.appendChild(button('edit', () => edit(member, binding), busy || !!pending));
          controls.appendChild(button('issue', () => preview(planFor(member, binding), true), busy || !!pending));
          for (const cred of binding.credentials || []) {
            const c = element('div', null, 'admin-credential');
            const active = !cred.revoked_at_unix_ms && (!cred.expires_at_unix_ms || cred.expires_at_unix_ms > Date.now());
            c.appendChild(element('span', cred.id)); c.appendChild(element('span', t(cred.revoked_at_unix_ms ? 'revoked' : active ? 'active' : 'expired'), 'badge'));
            if (!cred.project_scoped) c.appendChild(element('small', t('legacy')));
            else if (active) {
              const revokePlan = () => ({ ...planFor(member, binding), revoke_project_credentials: [cred.id] });
              c.appendChild(button('rotate', () => preview(revokePlan(), true), busy || !!pending));
              c.appendChild(button('revoke', () => preview(revokePlan(), false), busy || !!pending));
            }
            controls.appendChild(c);
          }
          row.appendChild(controls);
        }
        row.appendChild(button('remove', () => preview({ ...planFor(member, bindings[0]), grants: [], remove_membership: true }, false), busy || !!pending));
        list.appendChild(row);
      }
      host.appendChild(list);
      if (!members.length) host.appendChild(element('p', t(busy ? 'loading' : 'empty'), 'sub'));
      if (nextMember) host.appendChild(button('more', () => loadMembers(true), busy));
    }
    function renderActivity(host) {
      host.appendChild(element('h2', t(context.identity.can_read_project_audit ? 'project_scope' : 'personal')));
      host.appendChild(element('p', t('audit_note'), 'sub'));
      const filters = element('div', null, 'admin-toolbar');
      const kind = element('select'); kind.setAttribute('aria-label', t('activity'));
      for (const k of ['development', 'requests']) { const o = element('option', t(k)); o.value = k; kind.appendChild(o); }
      kind.value = auditKind; kind.disabled = busy; kind.addEventListener('change', async () => { auditKind = kind.value; activity = []; nextActivity = null; await loadActivity(); }); filters.appendChild(kind);
      if (context.identity.can_read_project_audit) {
        const m = element('input'); m.value = memberFilter; m.placeholder = t('filter_member'); m.setAttribute('aria-label', t('filter_member'));
        m.addEventListener('input', () => { memberFilter = m.value.trim(); }); filters.appendChild(m);
      }
      const w = element('input'); w.value = workFilter; w.placeholder = t('filter_work'); w.setAttribute('aria-label', t('filter_work'));
      w.addEventListener('input', () => { workFilter = w.value.trim(); }); filters.appendChild(w);
      filters.appendChild(button('filter', () => loadActivity(), busy)); host.appendChild(filters);
      const scroll = element('div', null, 'admin-table-scroll'), table = element('table', null, 'admin-table'), head = element('tr');
      for (const k of ['time', 'actor', 'client', 'action', 'work', 'result']) head.appendChild(element('th', t(k)));
      const thead = element('thead'); thead.appendChild(head); table.appendChild(thead);
      const body = element('tbody');
      for (const item of activity) {
        const tr = element('tr');
        for (const text of [item.created_at_unix_ms ? new Date(item.created_at_unix_ms).toLocaleString(i18n.locale) : '—', item.actor_id, item.client_id, item.action, item.work_id || '—', item.result || item.state]) tr.appendChild(element('td', value(text)));
        body.appendChild(tr);
      }
      table.appendChild(body); scroll.appendChild(table); host.appendChild(scroll);
      if (!activity.length) host.appendChild(element('p', t(busy ? 'loading' : 'empty'), 'sub'));
      if (nextActivity) host.appendChild(button('more', () => loadActivity(true), busy));
    }
    function render() {
      const tabs = $('teamConsoleTabs'), host = $('teamConsolePanel'), grid = $('teamWorkspaceGrid');
      if (!tabs || !host) return;
      tabs.textContent = ''; host.textContent = ''; tabs.hidden = !context; host.hidden = !context || tab === 'graph';
      if (!context) return;
      if (grid) grid.hidden = tab !== 'graph';
      tabs.setAttribute('role', 'tablist');
      for (const key of ['graph', ...(context.identity.can_manage_members ? ['members'] : []), 'activity']) {
        const b = button(key, async () => {
          tab = key; error = null; render();
          if (tab === 'members' && !loaded && !pending) await loadMembers();
          if (tab === 'activity' && !activityLoaded) await loadActivity();
        });
        b.setAttribute('role', 'tab'); b.setAttribute('aria-selected', String(tab === key)); b.classList.toggle('active', tab === key); tabs.appendChild(b);
      }
      if (error) host.appendChild(element('p', error.code + ': ' + error.message, 'admin-warning'));
      if (notice) { const n = element('p', notice, 'sub'); n.setAttribute('role', 'status'); host.appendChild(n); }
      if (tab === 'members') renderMembers(host);
      if (tab === 'activity') renderActivity(host);
    }
    return { setContext, reset, render };
  }
  const api = { createTeamAdmin };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_ADMIN = api;
})(typeof window !== 'undefined' ? window : globalThis);
