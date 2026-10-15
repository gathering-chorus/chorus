// @card: #4534
/* eslint-disable @typescript-eslint/require-await, sonarjs/no-duplicate-string -- test doubles model async injectors; repeated fixture literals are per-case readability (as service.test.ts) */
// @owner: wren
// @test-type: integration — supertest drives POST /api/nudge with real ES256 tokens signed by a test key
/**
 * #4534 — Abby runs as her own macOS user and cannot read the 0600 shared
 * secret, so every nudge she sent was refused. A sender may now prove itself
 * with its own identity token instead: ES256 from the trusted issuer, unexpired,
 * and its WebID must name the nudge's `from`.
 *
 * Negative proofs: no header, a forged signature, a token for another role,
 * an expired token and a foreign issuer are all still refused 403.
 */

import request from 'supertest';
import * as crypto from 'crypto';
import { MessageStore } from './store';
import { createApp, resetNudgeDedup } from './service';
import { configureCallerIdentity, roleFromWebId } from './caller-identity';

jest.mock('./peers', () => ({ fetchPeers: async () => ['abby-normal', 'jeff', 'kade', 'silas', 'wren'] }));

const ISSUER = 'https://id.example.test/';
const NOW = 2_000_000_000;
const KID = 'test-kid';
const { privateKey, publicKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });
const other = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });

function b64(o: object): string {
  return Buffer.from(JSON.stringify(o)).toString('base64url');
}

function token(claims: Record<string, unknown>, key: crypto.KeyObject = privateKey): string {
  const head = b64({ alg: 'ES256', kid: KID, typ: 'at+jwt' });
  const body = b64({ iss: ISSUER, exp: NOW + 600, ...claims });
  const sig = crypto.sign('sha256', Buffer.from(`${head}.${body}`), { key, dsaEncoding: 'ieee-p1363' });
  return `${head}.${body}.${sig.toString('base64url')}`;
}

const webId = (role: string) => `https://id.example.test/${role}/profile/card#me`;

beforeAll(() => {
  delete process.env.PULSE_ALLOW_DIRECT_POST;
  process.env.CHORUS_PULSE_SECRET = 'secret-the-sender-cannot-read';
  const jwk = { ...publicKey.export({ format: 'jwk' }), kid: KID };
  configureCallerIdentity({
    issuer: ISSUER,
    jwksUrl: 'http://jwks.test/.oidc/jwks',
    nowSecs: () => NOW,
    fetchFn: (async () => new Response(JSON.stringify({ keys: [jwk] }), { status: 200 })) as unknown as typeof fetch,
  });
});
afterAll(() => {
  delete process.env.CHORUS_PULSE_SECRET;
  configureCallerIdentity();
});
beforeEach(() => resetNudgeDedup());

function post(body: object, auth?: string) {
  const app = createApp(new MessageStore(':memory:'));
  const r = request(app).post('/api/nudge');
  return (auth ? r.set('Authorization', auth) : r).send(body);
}

const NUDGE = { from: 'abby-normal', to: 'wren', content: 'here' };

describe('#4534 a nudge sender proves itself with its own identity token', () => {
  it("Abby's own token, from=abby-normal, is accepted", async () => {
    const res = await post(NUDGE, `Bearer ${token({ webid: webId('abby-normal') })}`);
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
  });

  it('the shared secret still works for the roles that hold it', async () => {
    const app = createApp(new MessageStore(':memory:'));
    const res = await request(app).post('/api/nudge').set('X-Chorus-Pulse-Secret', 'secret-the-sender-cannot-read')
      .send({ from: 'wren', to: 'silas', content: 'still here' });
    expect(res.status).toBe(200);
  });

  it('NEGATIVE PROOF: no secret and no token is refused', async () => {
    const res = await post(NUDGE);
    expect(res.status).toBe(403);
    expect(res.body.error).toBe('not-canonical-caller');
  });

  it('NEGATIVE PROOF: a token signed by a key the issuer never published is refused', async () => {
    const res = await post(NUDGE, `Bearer ${token({ webid: webId('abby-normal') }, other.privateKey)}`);
    expect(res.status).toBe(403);
  });

  it("NEGATIVE PROOF: wren's valid token cannot send as abby-normal", async () => {
    const res = await post(NUDGE, `Bearer ${token({ webid: webId('wren') })}`);
    expect(res.status).toBe(403);
  });

  it('NEGATIVE PROOF: an expired token is refused', async () => {
    const res = await post(NUDGE, `Bearer ${token({ webid: webId('abby-normal'), exp: NOW - 1 })}`);
    expect(res.status).toBe(403);
  });

  it('NEGATIVE PROOF: a token from another issuer is refused', async () => {
    const res = await post(NUDGE, `Bearer ${token({ webid: webId('abby-normal'), iss: 'https://evil.test/' })}`);
    expect(res.status).toBe(403);
  });
});

describe('#4534 the WebID names the role', () => {
  it('reads the role from the profile path on the issuer host', () => {
    expect(roleFromWebId(webId('abby-normal'), ISSUER)).toBe('abby-normal');
  });
  it('NEGATIVE PROOF: another host, another path or no #me names no role', () => {
    expect(roleFromWebId('https://evil.test/abby-normal/profile/card#me', ISSUER)).toBeNull();
    expect(roleFromWebId('https://id.example.test/abby-normal/other#me', ISSUER)).toBeNull();
    expect(roleFromWebId('https://id.example.test/abby-normal/profile/card', ISSUER)).toBeNull();
  });
});
