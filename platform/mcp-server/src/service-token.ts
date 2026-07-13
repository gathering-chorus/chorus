/**
 * #3619 — scoped service-token mint for mcp-server's chorus-api writes
 * (doc-catalog/add is envelope-secured). Mirrors chorus-sdk/src/token.ts —
 * mcp-server has no chorus-sdk dependency and adding one for 60 lines was
 * judged heavier than the mirror; if the shape changes, change both (same
 * contract as the werk-demo/chorus-hooks memory-floor mirror).
 * Fail-open: no secret → null → caller sends bare and the envelope decides.
 */
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

function resolveSecret(): string | null {
  if (process.env.CHORUS_SERVICE_TOKEN_SECRET) return process.env.CHORUS_SERVICE_TOKEN_SECRET;
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- env CHORUS_REALM_ENV_PATH (operator-set) or the fixed ~/.chorus/secrets/chorus-realm.env; no user input (#3639)
    const raw = fs.readFileSync(
      process.env.CHORUS_REALM_ENV_PATH ||
        path.join(os.homedir(), '.chorus', 'secrets', 'chorus-realm.env'),
      'utf8'
    );
    for (const line of raw.split('\n')) {
      // eslint-disable-next-line security/detect-unsafe-regex -- anchored single-line matcher over trimmed env lines; linear, no nested quantifier backtracking (#3639)
      const m = /^(?:export\s+)?CHORUS_SERVICE_TOKEN_SECRET=(.+)$/.exec(line.trim());
      if (m) return m[1].replace(/^["']|["']$/g, '');
    }
  } catch { /* fail open */ }
  return null;
}

export function mintServiceToken(scopes: string[]): string | null {
  const secret = resolveSecret();
  if (!secret) return null;
  const b64 = (s: Buffer | string) => Buffer.from(s).toString('base64url');
  const header = b64(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const claims = b64(JSON.stringify({
    webId: process.env.CHORUS_AGENT_WEBID ||
      'http://localhost:3000/pods/chorus/_agents/chorus-sdk/profile/card.ttl#me',
    aud: 'chorus',
    exp: Math.floor(Date.now() / 1000) + 300,
    scope: scopes,
  }));
  const sig = b64(crypto.createHmac('sha256', secret).update(`${header}.${claims}`).digest());
  return `${header}.${claims}.${sig}`;
}
