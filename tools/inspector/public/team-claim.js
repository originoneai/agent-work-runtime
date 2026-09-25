/** Durable browser-to-MCP claim handoff. This never dispatches an Agent. */
(function (root) {
  'use strict';
  function error(code, message) { return Object.assign(new Error(message || code), { code }); }
  function createClaimFlow({ query, command, save, uuid, current = () => true }) {
    function guard() { if (!current()) throw error('SelectionChanged'); }
    async function read(op, scope, extra = {}) {
      guard();
      const result = await query({ protocol_version: 1, op, work_id: scope.work,
        workstream_id: scope.stream, ...extra });
      guard();
      return result;
    }
    async function inspect(scope, attempt) {
      if (!attempt.claim) return null;
      const result = await read('claim.inspect', scope, { claim_id: attempt.claim.claim_id,
        session_id: attempt.session.session_id });
      const lease = result.data;
      if (!lease || !lease.owned_by_client) throw error('ForeignClaim');
      return lease;
    }
    async function run(scope, attempt) {
      // Save only non-secret command metadata. Persist before sending any write.
      async function persist() { guard(); await save(attempt); guard(); }
      async function step(name, op, makeArgs) {
        if (attempt[name]) return attempt[name];
        const pending = name + 'Command';
        if (!attempt[pending]) {
          const prepared = await read('work.prepare', scope);
          const d = prepared.data;
          if (!d || d.work_id !== scope.work || prepared.workstream_id !== scope.stream)
            throw error('InvalidResponse');
          if (d.contract_hash !== scope.contract) throw error('SourceChanged');
          if (!d.context_complete || (d.runtime && d.runtime.recovery_blocked))
            throw error('ContextIncomplete');
          const args = await makeArgs(d);
          attempt[pending] = { protocol_version: 1, request_id: uuid(), op,
            workstream_id: scope.stream, work_id: scope.work,
            coordinator_epoch: prepared.coordinator_epoch,
            expected_project_revision: prepared.project_revision,
            expected_authority_version: prepared.authority_version,
            expected_ownership_version: d.ownership_version,
            expected_contract_hash: d.contract_hash, args };
          await persist();
        }
        const envelope = attempt[pending];
        const outcome = await read('command.inspect', scope, { request_id: envelope.request_id });
        let receipt = outcome.data && outcome.data.receipt;
        if (!receipt) {
          guard();
          try { receipt = (await command(envelope)).receipt; }
          catch (e) {
            // Only a definite transactional refusal permits a newly prepared intent.
            // Transport failure or an unreadable response retains the exact payload.
            if (['PreconditionsChanged', 'ContractChanged', 'SourceChanged', 'ClaimConflict',
              'LeaseConflict', 'ClaimHeld', 'WaitOpen', 'ContextIncomplete', 'RecoveryBlocked', 'Forbidden'].includes(e.code)) {
              delete attempt[pending]; await persist();
            }
            throw e;
          }
          guard();
        }
        if (!receipt || receipt.op !== op || receipt.request_id !== envelope.request_id || !receipt.data)
          throw error('InvalidResponse');
        attempt[name] = receipt.data;
        await persist();
        return attempt[name];
      }
      await step('session', 'session.start', async () => ({ conversation_id: 'inspector:' + uuid() }));
      await step('claim', 'claim.acquire', async d => {
        const s = await read('session.inspect', scope, { session_id: attempt.session.session_id });
        const session = (s.data.items || []).find(x => x.session_id === attempt.session.session_id);
        if (!session || session.state !== 'active') throw error('SessionEnded');
        return { session_id: session.session_id, expected_session_version: session.session_version,
          expected_work_version: d.runtime ? d.runtime.work_version : '0', ttl_seconds: 3600 };
      });
      return inspect(scope, attempt);
    }
    return { run, inspect };
  }
  const api = { createClaimFlow };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_CLAIM = api;
})(typeof window !== 'undefined' ? window : globalThis);
