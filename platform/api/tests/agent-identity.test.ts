// @test-type: unit
import { verifyAgentIdentity } from '../src/handlers/agent-identity';
import { createIdentityVerifier } from '../src/es256-identity';
import { generateKeyPairSync, sign } from 'node:crypto';

const { publicKey, privateKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
const issuer = 'https://identity.example/';
const principal = 'https://identity.example/opaque-principal';
function token(exp = 2000): string {
  const header = Buffer.from(JSON.stringify({ alg: 'ES256', kid: 'test' })).toString('base64url');
  const payload = Buffer.from(JSON.stringify({ iss: issuer, webid: principal, exp, role: 'jeff' })).toString('base64url');
  const input = `${header}.${payload}`;
  return `${input}.${sign('sha256', Buffer.from(input), { key: privateKey, dsaEncoding: 'ieee-p1363' }).toString('base64url')}`;
}
const verify = createIdentityVerifier({
  issuer, jwksUrl: `${issuer}.oidc/jwks`, scopeQuery: 'scope-query', nowSecs: () => 1000,
  fetchFn: (async () => ({ ok: true, json: async () => ({ keys: [{ ...publicKey.export({ format: 'jwk' }), kid: 'test' }] }) })) as typeof fetch,
  sparql: async () => ({ results: { bindings: [{ v: { value: `${principal} urn:chorus:index` } }] } }),
});

test('CSS signature plus model role resolves the actor; token role claim is ignored', async () => {
  const result = await verifyAgentIdentity(`Bearer ${token()}`, { verify, roleForWebId: async (webId) => webId === principal ? 'kade' : null });
  expect(result).toEqual({ status: 200, body: { ok: true, principal, role: 'kade', scopes: ['urn:chorus:index'] } });
  expect(JSON.stringify(result)).not.toContain(token());
});
test('missing, invalid, expired credentials never resolve a role', async () => {
  const roleForWebId = jest.fn();
  for (const authorization of ['', 'Bearer bad', `Bearer ${token(999)}`]) {
    expect((await verifyAgentIdentity(authorization, { verify, roleForWebId })).status).toBe(401);
  }
  expect(roleForWebId).not.toHaveBeenCalled();
});
test('unmapped identity refuses; unavailable identity dependencies fail closed', async () => {
  expect((await verifyAgentIdentity(`Bearer ${token()}`, { verify, roleForWebId: async () => null })).status).toBe(403);
  expect(await verifyAgentIdentity(`Bearer ${token()}`, { verify, roleForWebId: async () => { throw new Error('offline'); } }))
    .toEqual({ status: 503, body: { ok: false, error: 'identity-unavailable' } });
});
