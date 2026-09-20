// @test-type: unit
// #4202 regression: a shared HTTP daemon is not a role session. No token-file
// shape check may substitute its ambient credential for the authenticated caller.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { requestEnvironment, withAgentIdentity } from '../src/request-identity';

const daemon = { CHORUS_SESSION_TOKEN_FILE: '/shared/daemon.jwt', CHORUS_IDENTITY_TOKEN: 'daemon-secret', PATH: '/bin' };
test('tokenless request never inherits a daemon token or token file', () => {
  const env = withAgentIdentity({ mode: 'legacy-claude', role: 'wren' }, () => requestEnvironment(daemon));
  assert.equal(env.CHORUS_IDENTITY_TOKEN, undefined);
  assert.equal(env.CHORUS_SESSION_TOKEN_FILE, undefined);
  assert.equal(env.PATH, '/bin');
});
test('verified request supplies its own credential, not the daemon credential', () => {
  const env = withAgentIdentity({ mode: 'verified', role: 'kade', principal: 'https://identity.test/kade', scopes: [], token: 'request-token' }, () => requestEnvironment(daemon));
  assert.equal(env.CHORUS_IDENTITY_TOKEN, 'request-token');
  assert.equal(env.CHORUS_SESSION_TOKEN_FILE, undefined);
  assert.equal(env.CHORUS_ROLE, 'kade');
});
