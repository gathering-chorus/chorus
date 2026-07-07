// @test-type: unit — token minting with a fixture secret via env; no live
// services, no real secret file (path override points into a tmp dir).
/**
 * #3619 — service-token mint for SDK callers (the credential half of
 * deploy-before-require). Jeff's experience under test: when an endpoint's
 * gate flips, the SDK's trace POSTs keep flowing because they already carry
 * a valid scoped token; when no secret is available the SDK fails OPEN
 * (returns null, callers send unauthenticated) — a missing secret must never
 * break emit's fire-and-forget contract.
 */
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { mintServiceToken } from '../src/token';

const SECRET = 'sdk-test-secret-3619';

function decode(token: string) {
  const [h, c, s] = token.split('.');
  const expected = Buffer.from(
    crypto.createHmac('sha256', SECRET).update(`${h}.${c}`).digest()
  ).toString('base64url');
  return { claims: JSON.parse(Buffer.from(c, 'base64url').toString()), sigOk: s === expected };
}

describe('mintServiceToken', () => {
  afterEach(() => {
    delete process.env.CHORUS_SERVICE_TOKEN_SECRET;
    delete process.env.CHORUS_REALM_ENV_PATH;
  });

  test('mints a valid HS256 token with the requested scope', () => {
    process.env.CHORUS_SERVICE_TOKEN_SECRET = SECRET;
    const tok = mintServiceToken(['urn:chorus:ops']);
    expect(tok).not.toBeNull();
    const { claims, sigOk } = decode(tok!);
    expect(sigOk).toBe(true);
    expect(claims.aud).toBe('chorus');
    expect(claims.scope).toEqual(['urn:chorus:ops']);
    expect(claims.exp).toBeGreaterThan(Date.now() / 1000);
    expect(typeof claims.webId).toBe('string');
  });

  test('reads the secret from the realm env file when env var is absent', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'realm-'));
    const realm = path.join(dir, 'chorus-realm.env');
    fs.writeFileSync(realm, `# comment\nexport CHORUS_SERVICE_TOKEN_SECRET=${SECRET}\n`);
    process.env.CHORUS_REALM_ENV_PATH = realm;
    const tok = mintServiceToken(['urn:chorus:index']);
    expect(tok).not.toBeNull();
    expect(decode(tok!).sigOk).toBe(true);
    fs.rmSync(dir, { recursive: true, force: true });
  });

  test('fails OPEN (null) when no secret anywhere — never throws', () => {
    process.env.CHORUS_REALM_ENV_PATH = '/nonexistent/realm.env';
    expect(mintServiceToken(['urn:chorus:ops'])).toBeNull();
  });
});
