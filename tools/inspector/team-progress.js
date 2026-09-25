'use strict';

// Public GitHub observations are separate from AWR registration and acceptance.
// Only an exact GitHub PR reference from an authorized work response is followed.
function pullRequestReference(observation) {
  const registered = (observation.pr_deliveries || []).find(p => p.state === 'active' && p.contract_matches_current);
  const checkpoint = observation.checkpoint;
  const text = registered ? registered.url : checkpoint && checkpoint.contract_matches_current
    ? [checkpoint.next_action, ...(checkpoint.open_loops || [])].join('\n') : '';
  const url = String(text || '').match(/https:\/\/github\.com\/([\w.-]+)\/([\w.-]+)\/pull\/([1-9]\d*)(?=$|[\s/?#).,])/);
  const shorthand = !registered && String(text || '').match(/(?:^|[\s(])([\w.-]+)\/([\w.-]+)#([1-9]\d*)(?=$|[\s).,])/);
  const match = url || shorthand;
  if (!match) return null;
  const [, owner, repo, number] = match;
  if ([owner, repo].some(s => s === '.' || s === '..') || !Number.isSafeInteger(Number(number))) return null;
  return { owner, repo, number: Number(number), url: `https://github.com/${owner}/${repo}/pull/${number}`,
    registration: registered ? 'registered' : 'checkpoint_mention', expected_head: registered?.head_sha || null };
}

function mapObservation(data) {
  const session = data.session || {}, owner = data.responsibility || {};
  const execution = data.execution, checkpoint = data.checkpoint, claim = data.claim;
  const runtime = data.runtime || {};
  const terminal = ['completed', 'cancelled', 'archived'].includes(runtime.state);
  const attention = terminal ? null : runtime.recovery_blocked || execution?.recovery_blocked
    ? 'recovery_required' : claim && ['active', 'expired'].includes(claim.state) && !claim.lease_live
      ? 'lease_expired' : execution?.state === 'unknown' ? 'result_unknown' : null;
  return {
    observation_available: true, observed_at_ms: data.observed_at_unix_ms,
    last_activity_at_ms: data.last_activity_at_unix_ms,
    owner_person: owner.owner_name || owner.owner_person_id || null,
    claimant: session.actor_name || session.actor_id || null,
    agent: owner.executor_agent_id || null, client_id: session.client_id || null,
    model: data.model, usage: data.usage, session_id: session.id || null,
    checkpoint, claim, execution: execution ? {
      state: execution.state, id: execution.execution_id,
      contract_matches_current: execution.contract_matches_current,
      lease_live: execution.lease_live, receipt_details_available: execution.receipt_details_available,
      report: execution.latest_receipt ? {
        kind: execution.latest_receipt.receipt_kind,
        outcome: execution.latest_receipt.payload?.outcome,
        note: execution.latest_receipt.payload?.note,
      } : null,
    } : null,
    attention, status: runtime.state ?? null,
    next_step: checkpoint?.contract_matches_current ? checkpoint.next_action : null,
    pr_reference: pullRequestReference(data), missing: data.missing || {},
  };
}

function createGithubObserver(fetchImpl = fetch, now = Date.now) {
  const cache = new Map();
  async function read(path) {
    const response = await fetchImpl('https://api.github.com' + path, {
      headers: { accept: 'application/vnd.github+json', 'user-agent': 'AWR-Inspector' },
      redirect: 'error', signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) throw new Error('GitHub observation unavailable');
    return response.json();
  }
  return async function observe(reference) {
    if (!reference) return null;
    const key = reference.url, hit = cache.get(key);
    if (hit && now() - hit.at < 60000) return hit.promise;
    // The cache contains public response metadata only, and stays bounded.
    if (cache.size >= 200) cache.delete(cache.keys().next().value);
    const promise = (async () => {
      try {
        const base = `/repos/${encodeURIComponent(reference.owner)}/${encodeURIComponent(reference.repo)}`;
        const pr = await read(`${base}/pulls/${reference.number}`);
        if (!/^[a-f0-9]{40}$/.test(pr.head?.sha || '') || pr.html_url !== reference.url) throw new Error('Invalid PR');
        let ci = 'unavailable';
        try {
          const [checks, statuses] = await Promise.all([
            read(`${base}/commits/${pr.head.sha}/check-runs?per_page=100`),
            read(`${base}/commits/${pr.head.sha}/status?per_page=100`),
          ]);
          const runs = checks.check_runs || [], contexts = statuses.statuses || [];
          const incomplete = checks.total_count > runs.length || statuses.total_count > contexts.length;
          if (incomplete) ci = 'incomplete';
          else if (runs.some(r => ['failure', 'timed_out', 'cancelled', 'action_required', 'startup_failure', 'stale'].includes(r.conclusion))
            || contexts.some(s => ['failure', 'error'].includes(s.state))) ci = 'failed';
          else if (runs.some(r => r.status !== 'completed') || contexts.some(s => s.state === 'pending')) ci = 'pending';
          else if (runs.length + contexts.length === 0) ci = 'not_reported';
          else if (runs.every(r => ['success', 'neutral', 'skipped'].includes(r.conclusion)) && contexts.every(s => s.state === 'success')) ci = 'passed';
        } catch (_) { /* Keep PR facts when the independent checks request fails. */ }
        return { source: 'github_public_api', observed_at_ms: now(), url: reference.url,
          head_sha: pr.head.sha, state: pr.merged_at ? 'merged' : pr.state, ci };
      } catch (_) { return { source: 'github_public_api', observed_at_ms: now(), unavailable: true }; }
    })();
    cache.set(key, { at: now(), promise });
    return promise;
  };
}

module.exports = { mapObservation, pullRequestReference, createGithubObserver };
