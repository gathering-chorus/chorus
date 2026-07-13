/**
 * #3619 — scoped service-token mint for SDK callers.
 *
 * Same token shape as platform/scripts/chorus-mint-token.py (the one realm):
 * HS256, claims {webId, aud:"chorus", exp, scope:[...]}, signed with
 * CHORUS_SERVICE_TOKEN_SECRET. The chorus-api security envelope (#3618)
 * verifies signature + exp + scope on secured surfaces.
 *
 * Secret resolution: env CHORUS_SERVICE_TOKEN_SECRET, else the realm env file
 * (~/.chorus/secrets/chorus-realm.env; CHORUS_REALM_ENV_PATH overrides for
 * tests). Fail-OPEN: no secret → null — the caller sends unauthenticated and
 * the envelope decides. emit's fire-and-forget contract must never break on
 * a missing local secret.
 */
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const DEFAULT_WEBID =
  'http://localhost:3000/pods/chorus/_agents/chorus-sdk/profile/card.ttl#me';
const TTL_SECS = 300;

function realmPath(): string {
  return (
    process.env.CHORUS_REALM_ENV_PATH ||
    path.join(os.homedir(), '.chorus', 'secrets', 'chorus-realm.env')
  );
}

function resolveSecret(): string | null {
  const fromEnv = process.env.CHORUS_SERVICE_TOKEN_SECRET;
  if (fromEnv) return fromEnv;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- realmPath() is env CHORUS_REALM_ENV_PATH (operator-set) or the fixed ~/.chorus/secrets/chorus-realm.env; no user input (#3639)
    const raw = fs.readFileSync(realmPath(), 'utf8');
    for (const line of raw.split('\n')) {
      // eslint-disable-next-line security/detect-unsafe-regex -- anchored single-line matcher over trimmed env lines; linear, no nested quantifier backtracking (#3639)
      const m = /^(?:export\s+)?CHORUS_SERVICE_TOKEN_SECRET=(.+)$/.exec(line.trim());
      if (m) return m[1].replace(/^["']|["']$/g, '');
    }
  } catch {
    /* fail open */
  }
  return null;
}

function b64url(input: Buffer | string): string {
  return Buffer.from(input).toString('base64url');
}

/** Mint a scoped HS256 service token, or null when no secret is resolvable. */
export function mintServiceToken(scopes: string[]): string | null {
  const secret = resolveSecret();
  if (!secret) return null;
  const header = b64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64url(
    JSON.stringify({
      webId: process.env.CHORUS_AGENT_WEBID || DEFAULT_WEBID,
      aud: 'chorus',
      exp: Math.floor(Date.now() / 1000) + TTL_SECS,
      scope: scopes,
    })
  );
  const sig = b64url(crypto.createHmac('sha256', secret).update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}
