/**
 * #2474 — chorus-hook-shim binary location resolver.
 *
 * Single source of truth for "where is chorus-hook-shim on this host?"
 * Bash callers source platform/scripts/lib/resolve-shim.sh; TS callers go
 * through this function. The parity test (shim-path-parity.test.ts) pins the
 * two to the same order.
 *
 * Resolution order (#3750 — deploy artifact FIRST, per #2734: ~/.chorus/bin is
 * what RUNS; target/release is what was last COMPILED. Probing the build path
 * gave the 2026-08-04 false "shim missing" health fire and a suite that tested
 * a stale pre-membrane binary):
 *   1. CHORUS_SHIM_BIN env — explicit override (CI, alternate builds).
 *   2. $HOME/.chorus/bin/chorus-hook-shim — the deployed binary, when present.
 *   3. CHORUS_ROOT-derived build path — pre-deploy fallback.
 *   4. __dirname-relative absolute path — final fallback when no env is set.
 */
import * as fs from 'fs';
import * as path from 'path';

const SHIM_REL = 'platform/services/chorus-hooks/target/release/chorus-hook-shim';

export function resolveShimPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CHORUS_SHIM_BIN) return env.CHORUS_SHIM_BIN;
  // Pure function of env: HOME comes from the env arg (parity with bash $HOME),
  // never os.homedir(), so tests can construct the deployed-binary scenario.
  if (env.HOME) {
    const deployed = path.join(env.HOME, '.chorus', 'bin', 'chorus-hook-shim');
    if (fs.existsSync(deployed)) return deployed;
  }
  if (env.CHORUS_ROOT) return path.join(env.CHORUS_ROOT, SHIM_REL);
  // platform/api/src/shim-path.ts → ../../services/chorus-hooks/...
  return path.resolve(__dirname, '..', '..', 'services', 'chorus-hooks', 'target', 'release', 'chorus-hook-shim');
}
