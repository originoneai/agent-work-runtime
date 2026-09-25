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
    function button(label, action, disabled = false, style = '') {
      const b = element('button', t(label), 'btn ' + style); b.type = 'button'; b.disabled = disabled;
      b.addEventListener('click', async () => { if (!b.disabled) await action(); }); return b;
    }
    const roleKey = role => ({ admin: 'project_admin', worker: 'developer' })[role] || role;
    const streamName = id => { const s = streams.find(s => s.id === id); return s ? s.title || s.display_name || s.external_key || s.id : id; };
    const date = timestamp => new Date(timestamp).toLocaleDateString(i18n.locale);
    function focusPanel() { const panel = $('teamConsolePanel'); if (panel.scrollIntoView) panel.scrollIntoView({ behavior: 'smooth', block: 'start' }); }
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
      const same = context && context.project === next.project && context.session === next.session
        && context.identity.can_manage_members === next.identity.can_manage_members;
      if (!same) reset();
      context = next;
      if (!context.identity.can_manage_members) {
        if (tab === 'members') tab = 'graph'; members = []; editor = null; pending = null;
      }
      render();
    }
    async function access(operation, payload) {
      try {
        return await api('/api/team/access', { method: 'POST', body: JSON.stringify({
          project: context.project, operation, payload: { protocol_version: 1, ...payload },
        }) });
      } catch (_) { return { ok: false, error: { code: 'BridgeUnreachable' } }; }
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
          attest_execution: false, reconcile_execution: false,
        })), remove_membership: false, revoke_tenant_credentials: [] };
    }
    async function preview(plan, issue, action = 'edit', submit = false) {
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
            requestId: 'access-' + root.crypto.randomUUID(), status: 'preview', action, draft: editor };
          editor = null;
        }
      } catch (e) { if (g === generation) error = { code: 'CredentialPreparationFailed', message: String(e.message) }; }
      if (g !== generation) return;
      busy = false;
      if (pending && submit) await apply();
      render(); focusPanel();
    }
    async function apply() {
      if (busy || !pending || !['preview', 'retry'].includes(pending.status)) return;
      const g = generation, p = pending; busy = true; error = null; p.status = 'sending'; render();
      const body = await access('apply', { plan: p.plan, request_id: p.requestId,
        expected_state: p.preview.state_digest, expected_plan: p.preview.plan_digest });
      if (g !== generation) return;
      busy = false;
      if (body.ok) await finish(p);
      else { p.status = ['Forbidden', 'InvalidInput', 'PreconditionsChanged', 'IdempotencyConflict'].includes(body.error && body.error.code) ? 'rejected' : 'unknown'; fail(body); }
      render();
    }
    async function finish(p) {
      p.status = 'committed'; notice = t(p.action === 'create' ? 'member_created' : 'committed', { name: p.plan.subject.display_name }); loaded = false;
      if (!p.bearer) {
        pending = null;
        members = [];
        await loadMembers();
      }
    }
    async function inspectOutcome() {
      if (busy || !pending) return;
      const g = generation, p = pending; busy = true; error = null; render();
      const body = await access('outcome', { request_id: p.requestId });
      if (g !== generation) return;
      busy = false;
      if (!body.ok) fail(body);
      else if (body.data.outcome === 'committed') await finish(p);
      else p.status = 'retry';
      render();
    }
    function field(host, label, input) {
      const l = element('label', null, 'admin-field'); l.appendChild(element('span', t(label))); l.appendChild(input); host.appendChild(l); return input;
    }
    function renderEditor(host) {
      const form = element('form', null, 'admin-editor');
      form.appendChild(element('h3', t(editor.member ? 'edit' : 'add'), 'admin-full'));
      form.appendChild(element('p', t(editor.member ? 'edit_note' : 'create_note'), 'sub admin-full'));
      form.addEventListener('submit', async e => { e.preventDefault(); await submitEditor(form); });
      const name = element('input'); name.value = editor.name; name.required = true; name.maxLength = 200; name.disabled = !!editor.member;
      name.placeholder = t('name_placeholder'); name.autocomplete = 'off';
      name.addEventListener('input', () => { editor.name = name.value; }); field(form, 'name', name);
      const role = element('select');
      for (const key of ['reader', 'developer', 'reviewer', 'maintainer', 'project_admin']) { const o = element('option', t(key)); o.value = key; role.appendChild(o); }
      const hint = element('p', t('role_' + editor.role), 'sub admin-full');
      role.value = editor.role; role.addEventListener('change', () => { editor.role = role.value; hint.textContent = t('role_' + role.value); }); field(form, 'role', role);
      form.appendChild(hint);
      const scopes = element('fieldset'); scopes.appendChild(element('legend', t('streams')));
      for (const stream of streams) {
        const l = element('label', null, 'admin-check'); const input = element('input'); input.type = 'checkbox'; input.checked = editor.scopes.includes(stream.id);
        input.addEventListener('change', () => { editor.scopes = input.checked ? [...editor.scopes, stream.id] : editor.scopes.filter(id => id !== stream.id); });
        l.appendChild(input); l.appendChild(element('span', streamName(stream.id))); scopes.appendChild(l);
      }
      scopes.appendChild(element('p', t('streams_note'), 'sub'));
      form.appendChild(scopes);
      const review = element('input'); review.type = 'checkbox'; review.checked = editor.review;
      review.addEventListener('change', () => { editor.review = review.checked; });
      const reviewLabel = element('label', null, 'admin-check admin-full'); reviewLabel.appendChild(review); reviewLabel.appendChild(element('span', t('review'))); form.appendChild(reviewLabel);
      const actions = element('div', null, 'admin-actions admin-full');
      actions.appendChild(button('cancel', () => { editor = null; error = null; render(); }, busy));
      actions.appendChild(button(busy ? 'saving' : editor.member ? 'save' : 'create', () => submitEditor(form), busy, 'primary'));
      form.appendChild(actions); host.appendChild(form);
    }
    async function submitEditor(form) {
        if (!editor || busy || pending) return;
        if (!editor.name.trim() || editor.name.length > 200) { error = { code: 'NameRequired' }; render(); return; }
        if (!editor.scopes.length) { error = { code: 'ScopeRequired' }; render(); return; }
        if (form.reportValidity && !form.reportValidity()) return;
        const plan = { protocol_version: 1, subject: { id: editor.id, display_name: editor.name.trim(), kind: editor.member ? editor.member.kind : 'human' },
          subject_client_id: editor.binding ? editor.binding.client_id : editor.id + '-agent', role: editor.role,
          independent_review: editor.review, grants: streams.filter(s => editor.scopes.includes(s.id)).map(s => ({
            workstream_id: s.id, authority_version: s.authority_version, read: true,
            write: editor.role !== 'reader' && s.write, manage: editor.role === 'project_admin',
            attest_execution: false, reconcile_execution: false,
          })), remove_membership: false, revoke_tenant_credentials: [] };
        await preview(plan, !editor.member, editor.member ? 'edit' : 'create', true);
    }
    function edit(member, binding) {
      if (pending || busy) return;
      editor = { member, binding, name: member ? member.display_name : '', id: member ? member.actor_id : 'member-' + root.crypto.randomUUID(),
        role: member ? roleKey(member.role) : 'developer', review: member ? member.independent_review : false,
        scopes: binding ? (binding.grants || []).filter(g => g.active).map(g => g.workstream_id) : streams.map(s => s.id) };
      error = null; notice = null; render(); focusPanel();
    }
    function renderPending(host) {
      if (!pending) return;
      const box = element('section', null, 'admin-change');
      if (pending.status === 'committed' && pending.bearer) {
        box.appendChild(element('h3', t('secret_title', { name: pending.plan.subject.display_name })));
        box.appendChild(element('p', t('handoff_note'), 'sub'));
        const steps = element('ol', null, 'admin-handoff-steps');
        for (const key of ['handoff_copy', 'handoff_send', 'handoff_agent']) steps.appendChild(element('li', t(key)));
        box.appendChild(steps);
        box.appendChild(element('p', t('secret_note', { date: date(pending.plan.credential.expires_at_unix_ms) }), 'sub'));
        const text = onboarding.instruction(i18n, context.project, context.mcpUrl, pending.bearer);
        const details = element('details'); details.appendChild(element('summary', t('instruction_details')));
        const area = element('textarea'); area.readOnly = true; area.value = text; area.rows = 10; area.setAttribute('aria-label', t('instruction_details')); details.appendChild(area);
        const actions = element('div', null, 'admin-actions');
        actions.appendChild(button('copy', async () => {
          try { await root.navigator.clipboard.writeText(text); notice = t('copied'); render(); }
          catch (_) { details.open = true; area.focus(); area.select(); }
        }, false, 'primary'));
        actions.appendChild(button('close', async () => { pending = null; render(); await loadMembers(); }));
        box.appendChild(actions); box.appendChild(details);
      } else {
        box.appendChild(element('h3', t(busy ? 'saving' : 'confirm_' + pending.action)));
        const plan = pending.plan;
        box.appendChild(element('p', plan.subject.display_name + ' · ' + t(roleKey(plan.role))));
        box.appendChild(element('p', t('confirm_' + pending.action + '_note'), 'sub'));
        if (['unknown', 'retry'].includes(pending.status)) {
          box.appendChild(element('p', t('unknown'), 'admin-warning')); box.appendChild(button('inspect', inspectOutcome, busy));
          if (pending.status === 'retry') box.appendChild(button('retry', apply, busy));
        } else if (pending.status !== 'rejected') box.appendChild(button('confirm_' + pending.action, apply, busy, 'primary'));
        if (['preview', 'rejected'].includes(pending.status)) box.appendChild(button(pending.draft ? 'back' : 'cancel', () => { editor = pending.draft; pending = null; error = null; render(); }, busy));
      }
      host.appendChild(box);
    }
    function renderMembers(host) {
      host.appendChild(element('h2', t('members'))); host.appendChild(element('p', t('member_note'), 'sub'));
      if (!editor && !pending) {
        const toolbar = element('div', null, 'admin-toolbar');
        toolbar.appendChild(button('add', () => edit(null, null), busy || !loaded, 'primary'));
        toolbar.appendChild(button('refresh', () => loadMembers(), busy)); host.appendChild(toolbar);
      }
      if (editor) renderEditor(host);
      renderPending(host);
      if (editor || pending) return;
      const help = element('details', null, 'admin-help'); help.appendChild(element('summary', t('access_help')));
      help.appendChild(element('p', t('lost_note'), 'sub')); help.appendChild(element('p', t('scope_note'), 'sub')); host.appendChild(help);
      const list = element('div', null, 'admin-members');
      for (const member of members) {
        const row = element('article', null, 'admin-member');
        row.appendChild(element('h3', member.display_name)); row.appendChild(element('p', t(roleKey(member.role)), 'admin-role'));
        const bindings = member.clients.length ? member.clients : [{ client_id: member.actor_id + '-agent', grants: [], credentials: [] }];
        for (const [index, binding] of bindings.entries()) {
          const controls = element('div', null, 'admin-binding');
          if (bindings.length > 1) controls.appendChild(element('h4', t('connection_number', { number: index + 1 })));
          const grants = (binding.grants || []).filter(g => g.active);
          const hasScope = grants.some(g => g.read || g.write || g.manage);
          controls.appendChild(element('p', t('member_scopes', { scopes: grants.map(g => streamName(g.workstream_id)).join(' / ') || t('no_scope') }), 'sub'));
          const credentials = binding.credentials || [];
          const hasActive = credentials.some(c => !c.revoked_at_unix_ms && (!c.expires_at_unix_ms || c.expires_at_unix_ms > Date.now()));
          controls.appendChild(element('p', t(!hasScope ? 'access_needs_scope' : hasActive ? 'access_ready' : 'access_missing'), 'admin-access-status'));
          controls.appendChild(button('issue', () => preview(planFor(member, binding), true, 'issue', true), busy || !hasScope, 'primary'));
          controls.appendChild(button('edit', () => edit(member, binding), busy, hasScope ? '' : 'primary'));
          const existing = element('details', null, 'admin-help'); existing.appendChild(element('summary', t('credentials')));
          existing.appendChild(element('p', t('lost_note'), 'sub'));
          for (const [credentialIndex, cred] of credentials.entries()) {
            const c = element('div', null, 'admin-credential');
            const active = !cred.revoked_at_unix_ms && (!cred.expires_at_unix_ms || cred.expires_at_unix_ms > Date.now());
            c.appendChild(element('span', t('credential_number', { number: credentialIndex + 1 }))); c.appendChild(element('span', t(cred.revoked_at_unix_ms ? 'revoked' : active ? 'active' : 'expired'), 'badge'));
            if (cred.expires_at_unix_ms) c.appendChild(element('small', t('valid_until', { date: date(cred.expires_at_unix_ms) })));
            if (!cred.project_scoped) c.appendChild(element('small', t('legacy')));
            else if (active) {
              const revokePlan = () => ({ ...planFor(member, binding), revoke_project_credentials: [cred.id] });
              c.appendChild(button('rotate', () => preview(revokePlan(), true, 'rotate'), busy));
              c.appendChild(button('revoke', () => preview(revokePlan(), false, 'revoke'), busy));
            }
            existing.appendChild(c);
          }
          if (!credentials.length) existing.appendChild(element('p', t('access_missing'), 'sub'));
          controls.appendChild(existing);
          const technical = element('details', null, 'admin-technical'); technical.appendChild(element('summary', t('technical')));
          technical.appendChild(element('p', member.actor_id + ' / ' + binding.client_id));
          controls.appendChild(technical);
          row.appendChild(controls);
        }
        row.appendChild(button('remove', () => preview({ ...planFor(member, bindings[0]), grants: [], remove_membership: true }, false, 'remove'), busy, 'admin-danger'));
        list.appendChild(row);
      }
      host.appendChild(list);
      if (!members.length) host.appendChild(element('p', t(busy ? 'loading' : 'empty'), 'sub'));
      if (nextMember) host.appendChild(button('more', () => loadMembers(true), busy));
    }
    function renderActivity(host) {
      host.appendChild(element('h2', t(context.identity.can_read_project_audit ? 'project_scope' : 'personal')));
      host.appendChild(element('p', t(context.identity.can_read_project_audit ? 'audit_note' : 'personal_note'), 'sub'));
      const filters = element('div', null, 'admin-toolbar');
      const kind = element('select'); kind.setAttribute('aria-label', t('activity'));
      for (const k of ['development', 'requests']) { const o = element('option', t(k)); o.value = k; kind.appendChild(o); }
      kind.value = auditKind; kind.disabled = busy; kind.addEventListener('change', async () => { auditKind = kind.value; activity = []; nextActivity = null; await loadActivity(); }); filters.appendChild(kind);
      if (context.identity.can_read_project_audit) {
        const m = element('select'); m.setAttribute('aria-label', t('member'));
        const everyone = element('option', t('all_members')); everyone.value = ''; m.appendChild(everyone);
        const known = new Map(activity.filter(a => a.actor_id).map(a => [a.actor_id, a.actor_id]));
        for (const member of members) known.set(member.actor_id, member.display_name);
        if (memberFilter && !known.has(memberFilter)) known.set(memberFilter, memberFilter);
        for (const [id, name] of known) { const option = element('option', name); option.value = id; m.appendChild(option); }
        m.value = memberFilter; m.disabled = busy;
        m.addEventListener('change', () => { memberFilter = m.value; }); filters.appendChild(m);
        if (nextMember) filters.appendChild(button('more_members', () => loadMembers(true), busy));
      }
      const w = element('input'); w.value = workFilter; w.placeholder = t('filter_work'); w.setAttribute('aria-label', t('filter_work'));
      w.addEventListener('input', () => { workFilter = w.value.trim(); }); filters.appendChild(w);
      filters.appendChild(button('filter', () => loadActivity(), busy)); host.appendChild(filters);
      const scroll = element('div', null, 'admin-table-scroll'), table = element('table', null, 'admin-table'), head = element('tr');
      for (const k of ['time', 'member', 'action', 'work', 'result']) head.appendChild(element('th', t(k)));
      const thead = element('thead'); thead.appendChild(head); table.appendChild(thead);
      const body = element('tbody');
      for (const item of activity) {
        const tr = element('tr');
        const member = members.find(m => m.actor_id === item.actor_id);
        for (const text of [item.created_at_unix_ms ? new Date(item.created_at_unix_ms).toLocaleString(i18n.locale) : '—', member ? member.display_name : item.actor_id, activityLabel(item.action), item.work_id || '—', activityLabel(item.result || item.state)]) tr.appendChild(element('td', value(text)));
        body.appendChild(tr);
      }
      table.appendChild(body); scroll.appendChild(table); host.appendChild(scroll);
      if (!activity.length) host.appendChild(element('p', t(busy ? 'loading' : 'empty'), 'sub'));
      if (nextActivity) host.appendChild(button('more', () => loadActivity(true), busy));
    }
    function activityLabel(raw) {
      const key = 'admin.event_' + raw;
      const label = i18n.t(key);
      return label === key ? raw : label;
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
          if (tab === 'activity' && !activityLoaded) {
            if (context.identity.can_manage_members && !loaded && !pending) await loadMembers();
            if (context && tab === 'activity') await loadActivity();
          }
        });
        b.setAttribute('role', 'tab'); b.setAttribute('aria-selected', String(tab === key)); b.classList.toggle('active', tab === key); tabs.appendChild(b);
      }
      if (error) {
        const key = 'admin.error_' + error.code;
        const translated = i18n.t(key);
        const warning = element('div', null, 'admin-warning'); warning.setAttribute('role', 'alert');
        warning.appendChild(element('p', translated === key ? t('error_generic') : translated));
        const detail = element('details'); detail.appendChild(element('summary', t('technical')));
        detail.appendChild(element('code', error.code)); warning.appendChild(detail); host.appendChild(warning);
      }
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
