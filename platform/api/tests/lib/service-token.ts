/**
 * #3619/#3719 — service auth for integration tests that hit the LIVE chorus-api.
 *
 * Every mutation endpoint is behind the security envelope; these suites are
 * real consumers (the nightly 03:0x write bursts in Loki were exactly them),
 * so they carry credentials like any other caller — deploy-before-require.
 *
 * #3719: the credential is a CSS ES256 identity token via chorus-identity-token
 * (the same minter every live caller uses). No scope claim — what the identity
 * may write is chorus:hasScope grants in the model (#3689); chorus-sdk's
 * Principal carries the ops grant. Fail-open: no credential → requests go bare
 * and the envelope decides (suites then fail visibly with 401s, naming exactly
 * what's missing).
 */
import { execFileSync } from 'child_process';
import * as os from 'os';
import * as path from 'path';

export function mintTestToken(): string | null {
  const role = process.env.CHORUS_ROLE || process.env.DEPLOY_ROLE || 'chorus-sdk';
  const bin =
    process.env.CHORUS_IDENTITY_TOKEN_BIN ||
    path.join(os.homedir(), 'CascadeProjects', 'chorus', 'platform', 'scripts', 'chorus-identity-token');
  try {
    const out = execFileSync(bin, [role], { timeout: 8000, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString('utf-8')
      .trim();
    return out.split('.').length === 3 ? out : null;
  } catch {
    return null;
  }
}

const WRITE = new Set(['POST', 'PUT', 'DELETE', 'PATCH']);

/** Wrap global fetch for the suite: mutating requests carry the test token. */
export function withServiceAuth(): void {
  const token = mintTestToken();
  if (!token) return; // fail open — envelope will 401 and the suite says why
  const real = globalThis.fetch;
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method || 'GET').toUpperCase();
    if (WRITE.has(method)) {
      init = { ...init, headers: { ...(init?.headers as Record<string, string>), Authorization: `Bearer ${token}` } };
    }
    return real(input as never, init);
  }) as typeof fetch;
}
