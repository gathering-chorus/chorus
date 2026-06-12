// Standalone embed worker entry (#3379) — the #3085 reindex-worker twin.
//
// Runs ONE embed-delta pass in its OWN process, NOT on chorus-api's event loop.
// The pass interleaves synchronous better-sqlite3 page reads with lance writes;
// in-process it blocked serving (the 2026-06-12 wedge class — 5 outages in one
// day, convicted by isolation: API calm at 2.6% CPU with the embed pass off,
// 65-100% CPU wedges with it on). index-worker.ts's old comment ("async Ollama
// I/O, which the loop tolerates in-process") was the assumption this day broke.
//
// Launched by chorus-embed-worker.sh on the com.chorus.embed-worker cadence;
// the launcher holds the lock and the Ollama pre-check.
import { makeEmbedDelta } from './embed-delta-deps';

export interface RunEmbedWorkerDeps {
  embedDelta: () => Promise<{ embedded: number; skipped: number; ollama_failures: number }>;
  log?: (m: string) => void;
  error?: (m: string) => void;
}

/** Testable core: run one embed pass, return process exit code (0 ok, 1 fail). */
export async function runEmbedWorker(deps: RunEmbedWorkerDeps): Promise<number> {
  const log = deps.log ?? ((m) => console.log(m));
  const error = deps.error ?? ((m) => console.error(m));
  try {
    const r = await deps.embedDelta();
    log(JSON.stringify({ embedded: r.embedded, skipped: r.skipped, ollama_failures: r.ollama_failures }));
    // Partial Ollama failures are content for the log, not a crash; a fully
    // failed pass (nothing embedded, failures present) is a real failure.
    if (r.embedded === 0 && r.ollama_failures > 0) {
      error(`[embed-worker] pass embedded nothing with ${r.ollama_failures} ollama failures`);
      return 1;
    }
    return 0;
  } catch (err) {
    error(`[embed-worker] FAIL — ${(err as Error).message}`);
    return 1;
  }
}

/* istanbul ignore next — process entry, exercised by the LaunchAgent */
if (require.main === module) {
  void runEmbedWorker({ embedDelta: () => makeEmbedDelta().run() }).then((code) => process.exit(code));
}
