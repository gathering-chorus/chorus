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

export function fileTaskCache(projectId: number): TaskCache {
  const file = () => path.join(cacheDir(), `tasks-${projectId}.json`);
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
