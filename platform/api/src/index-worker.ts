// Standalone reindex worker entry (#3085).
//
// Runs indexAllSources() in its OWN process, NOT on chorus-api's event loop.
// better-sqlite3 is synchronous, so a reindex pass blocks whatever event loop
// it runs on; keeping it inside chorus-api froze every request (the
// eventloop.blocked alerts). This worker is launched by chorus-reindex-worker.sh
// on a LaunchAgent cadence — the API process is never touched by reindex.
//
// Mirrors the chorus-embed-worker.sh model (separate process, lock-guarded by the
// launcher, restarted on interval) — but unlike embed (async Ollama I/O, which the
// loop tolerates in-process) reindex MUST run its compute out-of-process, so this
// runs the indexing directly rather than curling the API (#3080 Track A / ADR-034).
import path from 'path';
import os from 'os';
import { makeIndexAllSources } from './index-all-sources-deps';

export interface RunReindexWorkerDeps {
  indexAllSources: () => Promise<{ indexed: Record<string, string>; elapsed_ms: number }>;
  log?: (m: string) => void;
  error?: (m: string) => void;
}

/** Testable core: run one reindex pass, return process exit code (0 ok, 1 fail). */
export async function runReindexWorker(deps: RunReindexWorkerDeps): Promise<number> {
  const log = deps.log ?? ((m) => console.log(m));
  const error = deps.error ?? ((m) => console.error(m));
  try {
    const result = await deps.indexAllSources();
    const total = Object.keys(result.indexed).length;
    log(`[reindex-worker] complete — ${total} sources indexed in ${result.elapsed_ms}ms`);
    return 0;
  } catch (err: unknown) {
    error(`[reindex-worker] failed: ${err instanceof Error ? err.message : String(err)}`);
    return 1;
  }
}

// Entry: wire real deps + exit with the run's code. Same path resolution as
// server.ts (DB_PATH, REPO_ROOT) so the worker indexes the exact same stores.
if (require.main === module) {
  const dbPath = path.join(os.homedir(), '.chorus', 'index.db');
  const repoRoot = path.resolve(__dirname, '../../..');
  void runReindexWorker({ indexAllSources: makeIndexAllSources({ dbPath, repoRoot }) })
    .then((code) => process.exit(code));
}
