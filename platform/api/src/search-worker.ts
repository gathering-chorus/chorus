/**
 * #3382 — semantic-search worker PROCESS entry. Forked as a child of chorus-api
 * (NOT a worker_thread): lance's native CPU pool (lance-cpu) is process-global
 * and does continuous background work on the open table handle, so only a
 * separate OS process keeps it off chorus-api's event loop (the 2026-06-12/13
 * wedge storm — 7,327 eventloop.blocked events; each kickstart bought ~30min
 * because a fresh process re-saturated). The handle opens HERE, lazily, on the
 * first request — chorus-api never opens or scans lance again.
 *
 * Thin glue (lance open + embedder + IPC wiring). The query logic lives in
 * search-worker-core (unit-tested) and the dispatch/lifecycle in worker-pool
 * (unit-tested). CHORUS_LANCE_DIR is inherited from the parent, so a variant
 * chorus-api (env-up #3381) forks its own worker at its own dir — no separate
 * endpoint discovery needed.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import * as lancedb from '@lancedb/lancedb';
import { createLanceInit, type VectorTable } from './lance-store';
import { createEmbedder } from './embed-query';
import { handleSearchMessage, type SearchRequest } from './search-worker-core';

const LANCE_DIR = process.env.CHORUS_LANCE_DIR || path.join(os.homedir(), '.chorus', 'lance');
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';
const EMBED_MODEL = process.env.CHORUS_EMBED_MODEL || 'nomic-embed-text';

const embed = createEmbedder({ ollamaUrl: OLLAMA_URL, model: EMBED_MODEL });

let table: VectorTable | null = null;
let ready: Promise<void> | null = null;
function ensureReady(): Promise<void> {
  if (!ready) {
    ready = createLanceInit({ fs, lancedb, lanceDir: LANCE_DIR })().then((r) => {
      table = r.table;
    });
  }
  return ready;
}

/* istanbul ignore next — process IPC glue, exercised by the live worker at deploy */
process.on('message', (msg: SearchRequest) => {
  void ensureReady()
    .then(() => handleSearchMessage({ table, embed }, msg))
    .then((reply) => process.send?.(reply))
    .catch((e) =>
      process.send?.({
        id: msg && typeof msg.id === 'number' ? msg.id : -1,
        error: e instanceof Error ? e.message : String(e),
      }),
    );
});
