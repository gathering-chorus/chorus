/**
 * #2682 — git-queue.sh path resolver. Mirrors cards-path.ts / shim-path.ts.
 * Single source of truth for "where is git-queue.sh on this host?" so the
 * MCP chorus_commit tool spawns the canonical script.
 *
 * Resolution order:
 *   1. CHORUS_GIT_QUEUE_BIN env — explicit override (tests, alt builds).
 *   2. CHORUS_ROOT env — same convention as the other path resolvers.
 *   3. __dirname-relative absolute path — final fallback.
 */
import * as path from 'path';

const GIT_QUEUE_REL = 'platform/scripts/git-queue.sh';

export function resolveGitQueuePath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CHORUS_GIT_QUEUE_BIN) return env.CHORUS_GIT_QUEUE_BIN;
  if (env.CHORUS_ROOT) return path.join(env.CHORUS_ROOT, GIT_QUEUE_REL);
  // platform/api/src/git-queue-path.ts → ../../scripts/git-queue.sh
  return path.resolve(__dirname, '..', '..', 'scripts', 'git-queue.sh');
}
