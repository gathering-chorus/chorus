/**
 * #2474 — chorus-hook-shim binary location resolver.
 *
 * Single source of truth for "where is chorus-hook-shim on this host?"
 * Bash callers (platform/scripts/nudge) use ${CHORUS_ROOT}/.../chorus-hook-shim
 * directly; TS callers go through this function so the convention matches.
 *
 * Resolution order:
 *   1. CHORUS_SHIM_BIN env — explicit override (CI, alternate builds).
 *   2. CHORUS_ROOT env — same convention bash CLI uses.
 *   3. __dirname-relative absolute path — final fallback when neither env
 *      is set (e.g., test runners launched from a clean shell).
 */
import * as path from 'path';

const SHIM_REL = 'platform/services/chorus-hooks/target/release/chorus-hook-shim';

export function resolveShimPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CHORUS_SHIM_BIN) return env.CHORUS_SHIM_BIN;
  if (env.CHORUS_ROOT) return path.join(env.CHORUS_ROOT, SHIM_REL);
  // platform/api/src/shim-path.ts → ../../services/chorus-hooks/...
  return path.resolve(__dirname, '..', '..', 'services', 'chorus-hooks', 'target', 'release', 'chorus-hook-shim');
}
