/**
 * #3719 — ES256 identity mint for mcp-server's chorus-api writes
 * (doc-catalog/add is envelope-secured). Mirrors chorus-sdk/src/token.ts —
 * mcp-server has no chorus-sdk dependency and adding one for 40 lines was
 * judged heavier than the mirror; if the shape changes, change both (same
 * contract as the werk-demo/chorus-hooks memory-floor mirror).
 *
 * The token is a CSS-issued ES256 identity token via chorus-identity-token
 * (#3690); scope is chorus:hasScope grants in the model, resolved at the
 * verifying door (#3689) — no scope claim, no shared secret.
 * Fail-open: no credential → null → caller sends bare and the envelope decides.
 */
import { execFileSync } from 'child_process';
import * as os from 'os';
import * as path from 'path';

export function mintServiceToken(): string | null {
  const role = process.env.CHORUS_ROLE || process.env.DEPLOY_ROLE || 'chorus-sdk';
  const bin =
    process.env.CHORUS_IDENTITY_TOKEN_BIN ||
    path.join(os.homedir(), 'CascadeProjects', 'chorus', 'platform', 'scripts', 'chorus-identity-token');
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- bin is env CHORUS_IDENTITY_TOKEN_BIN (operator-set) or the fixed repo script path; no user input
    const out = execFileSync(bin, [role], { timeout: 8000, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString('utf-8')
      .trim();
    return out.split('.').length === 3 ? out : null;
  } catch {
    return null;
  }
}
