/**
 * #3719 — ES256 identity mint for SDK callers (retires the #3619 HS256 mint).
 *
 * The token is a CSS-issued identity token minted via chorus-identity-token
 * (the #3690 cred-reader: per-agent credential → client_credentials → ES256
 * at+jwt, TTL-cached on disk). It carries NO scope claim — what the identity
 * may write is chorus:hasScope grants in the model, resolved at the verifying
 * door (#3689). chorus-sdk's Principal is granted urn:chorus:ops.
 *
 * Fail-OPEN contract unchanged: no credential / CSS down → null — the caller
 * sends unauthenticated and the envelope decides. emit's fire-and-forget
 * contract must never break on a missing local identity.
 */
import { execFileSync } from 'child_process';
import * as os from 'os';
import * as path from 'path';

const DEFAULT_ROLE = 'chorus-sdk';

function mintBin(): string {
  return (
    process.env.CHORUS_IDENTITY_TOKEN_BIN ||
    path.join(os.homedir(), 'CascadeProjects', 'chorus', 'platform', 'scripts', 'chorus-identity-token')
  );
}

/** Mint an ES256 identity token for this process's role, or null when no
 *  credential is resolvable (fail open — the envelope decides). */
export function mintServiceToken(): string | null {
  const role = process.env.CHORUS_ROLE || process.env.DEPLOY_ROLE || DEFAULT_ROLE;
  try {
    // #3721 — dropped the eslint-disable for security/detect-non-literal-fs-filename:
    // that rule targets fs.* calls and never fired on this execFileSync, so the
    // suppression was dead. A dead suppression is worse than none — it reads as
    // "a real finding was reviewed and accepted here", and it would silently
    // swallow the rule if it ever DID start matching. Its rationale is worth
    // keeping, so it stays as a plain comment: mintBin() resolves to env
    // CHORUS_IDENTITY_TOKEN_BIN (operator-set) or the fixed repo script path —
    // no user input reaches it.
    const out = execFileSync(mintBin(), [role], { timeout: 8000, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString('utf-8')
      .trim();
    return out.split('.').length === 3 ? out : null;
  } catch {
    return null;
  }
}
