/**
 * #3625 AC3 — short-TTL disk cache for the full-board task sweep.
 *
 * fetchAllTasks paginates the ENTIRE project (73+ pages, ~3,650 tasks) and
 * every cards invocation used to pay it. During the 2026-07-07 Library OOM,
 * concurrent agent turns drove 26 of those sweeps per minute into Vikunja
 * (9,800 req/5min) while the box was already spiraling. The cache is on
 * disk, not in-process, precisely so concurrent CLI invocations — separate
 * processes — share one sweep.
 *
 * Staleness contract: TTL is seconds (default 30s — the board is coordination
 * state, not a ledger; Vikunja's own UI polls slower). Same-process mutations
 * invalidate via BoardClient.clearCache(). Callers that need guaranteed-fresh
 * data (resolveIndex's full-scan fallback) bypass with fetchAllTasks(true).
 */
import * as fs from 'fs';
import * as os from 'os';
import * as crypto from 'crypto';
import * as path from 'path';
import { VikunjaTask } from './types';

export interface TaskCache {
  read(): VikunjaTask[] | null;
  write(tasks: VikunjaTask[]): void;
  invalidate(): void;
}

const DEFAULT_TTL_MS = 30_000;

function cacheDir(): string {
  return (
    process.env.CARDS_CACHE_DIR ||
    path.join(process.env.HOME || os.homedir(), '.chorus', 'cards-cache')
  );
}

/**
 * #4010 — the cache key carries the CALLER'S IDENTITY, not just the project.
 *
 * Before this, the key was `tasks-${projectId}.json` and `list()` read it
 * BEFORE any API call. A client built with a bad credential therefore returned
 * the entire board out of a cache someone else's authorized read had filled —
 * never contacting Vikunja, never seeing the 401 Vikunja was perfectly willing
 * to give. The client could not tell "you are allowed to see this" from "I
 * remember seeing this", which is the same defect class as a write door that
 * authenticates SOMEONE and then trusts the body for WHO.
 *
 * `identity` is a short SHA-256 prefix of the bearer token. The token itself is
 * never written, logged, or included in a path — only its digest, so a
 * different credential simply misses the cache and is forced to ask Vikunja,
 * which refuses it properly. An absent identity keys as "anon" rather than
 * silently sharing the authorized pool.
 */
export function fileTaskCache(projectId: number, identity?: string): TaskCache {
  const who = identity && identity.length ? identity : 'anon';
  const file = () => path.join(cacheDir(), `tasks-${projectId}-${who}.json`);
  const disabled = () => !!process.env.CARDS_CACHE_DISABLE;
  const ttlMs = () => Number(process.env.CARDS_CACHE_TTL_MS) || DEFAULT_TTL_MS;

  return {
    read(): VikunjaTask[] | null {
      if (disabled()) return null;
      try {
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is cacheDir() (env CARDS_CACHE_DIR or ~/.chorus/cards-cache) + `tasks-${projectId}.json` with a numeric projectId; no user input reaches the path (#3639)
        const raw = fs.readFileSync(file(), 'utf8');
        const parsed = JSON.parse(raw) as { ts: number; tasks: VikunjaTask[] };
        if (!Array.isArray(parsed.tasks)) return null;
        if (Date.now() - parsed.ts >= ttlMs()) return null;
        return parsed.tasks;
      } catch {
        return null; // absent, corrupt, unreadable — all read as a miss
      }
    },

    write(tasks: VikunjaTask[]): void {
      if (disabled()) return;
      try {
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is cacheDir() (env CARDS_CACHE_DIR or ~/.chorus/cards-cache) + `tasks-${projectId}.json` with a numeric projectId; no user input reaches the path (#3639)
        fs.mkdirSync(cacheDir(), { recursive: true });
        // Atomic: concurrent readers see either the old cache or the new one,
        // never a torn file.
        const tmp = `${file()}.${process.pid}.tmp`;
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is cacheDir() (env CARDS_CACHE_DIR or ~/.chorus/cards-cache) + `tasks-${projectId}.json` with a numeric projectId; no user input reaches the path (#3639)
        fs.writeFileSync(tmp, JSON.stringify({ ts: Date.now(), tasks }));
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is cacheDir() (env CARDS_CACHE_DIR or ~/.chorus/cards-cache) + `tasks-${projectId}.json` with a numeric projectId; no user input reaches the path (#3639)
        fs.renameSync(tmp, file());
      } catch {
        // Cache write failure is never an error path — the sweep succeeded.
      }
    },

    invalidate(): void {
      try {
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is cacheDir() (env CARDS_CACHE_DIR or ~/.chorus/cards-cache) + `tasks-${projectId}.json` with a numeric projectId; no user input reaches the path (#3639)
        fs.unlinkSync(file());
      } catch {
        // already absent
      }
    },
  };
}

/**
 * The cache identity for a bearer token: a short digest, never the token.
 * Exported so the client and its tests derive it the same way — two
 * derivations would be two identities, and the bug would come back wearing a
 * different key.
 */
export function cacheIdentity(token: string | undefined): string {
  if (!token) return 'anon';
  return crypto.createHash('sha256').update(token).digest('hex').slice(0, 12);
}
