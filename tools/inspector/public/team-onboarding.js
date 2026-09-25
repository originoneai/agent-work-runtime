/** One-time project credential generation. No persistence or network calls. */
(function (root) {
  'use strict';
  const hex = bytes => Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  async function generateCredential(cryptoApi) {
    if (!cryptoApi || !cryptoApi.subtle || !cryptoApi.getRandomValues) throw new Error('SecureContextRequired');
    const id = 'member-' + hex(cryptoApi.getRandomValues(new Uint8Array(12)));
    const bearer = 'awr1.' + id + '.' + hex(cryptoApi.getRandomValues(new Uint8Array(32)));
    const digest = await cryptoApi.subtle.digest('SHA-256', new TextEncoder().encode('awr-team-credential-v1:' + bearer));
    return { bearer, credential: { id, secret_hash: 'sha256:' + hex(new Uint8Array(digest)),
      expires_at_unix_ms: Date.now() + 90 * 24 * 60 * 60 * 1000 } };
  }
  function instruction(i18n, project, endpoint, bearer) {
    return i18n.t('admin.agent_instruction', { project, endpoint, bearer });
  }
  const api = { generateCredential, instruction };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_ONBOARDING = api;
})(typeof window !== 'undefined' ? window : globalThis);
