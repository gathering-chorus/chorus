// embed-delta-deps.ts — shared construction of the embed-delta pass (#3379)
//
// Mirrors index-all-sources-deps.ts (#3085): ONE wiring used by both the
// standalone embed worker (dist/embed-delta-worker.js — the only scheduled
// caller) and any in-process consumer, so the two can never drift
// (chorus:principle-no-competing-implementations).
//
// Why this exists: the embed pass interleaves synchronous better-sqlite3 page
// reads with lance writes. Run on chorus-api's event loop it blocks serving —
// the 2026-06-12 wedge class (5 outages, fs/sqlite storms, 2 manual
// kickstarts). The worker process eats the blocking; the API never does.
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import Database from 'better-sqlite3';
import * as lancedb from '@lancedb/lancedb';
import { createEmbedDelta, singleFlight } from './embed-delta';
import { createLanceInit } from './lance-store';
import { createEmbedder } from './embed-query';
import { MIN_EMBED_LENGTH } from './embed-floor'; // #2754 — single source of truth

const EMBED_PAGE_SIZE = 100; // one page per cycle; the LaunchAgent cadence handles the rest (#1920)

export interface EmbedDeltaRunner {
  run: () => Promise<{ embedded: number; skipped: number; ollama_failures: number }>;
}

/** Build a self-contained embed-delta runner with its own DB + lance handles. */
export function makeEmbedDelta(opts: { dbPath?: string; lanceDir?: string } = {}): EmbedDeltaRunner {
  const dbPath = opts.dbPath ?? path.join(os.homedir(), '.chorus', 'index.db');
  const lanceDir = opts.lanceDir ?? path.join(os.homedir(), '.chorus', 'lance');
  const ollamaBulkUrl = process.env.OLLAMA_BULK_URL || process.env.OLLAMA_URL || 'http://127.0.0.1:11434';
  const embedBulk = createEmbedder({ ollamaUrl: ollamaBulkUrl, model: 'nomic-embed-text' });

  const lanceInit = createLanceInit({ fs, lancedb, lanceDir });
  let lanceDb: unknown = null;
  let lanceTable: unknown = null;

  const inner = singleFlight(createEmbedDelta({
    dbPath,
    DatabaseCtor: Database,
    getLanceStore: () => ({
      db: lanceDb as { createTable: (n: string, rec: Record<string, unknown>[]) => Promise<unknown> } | null,
      table: lanceTable as { add: (rec: Record<string, unknown>[]) => Promise<void> } | null,
    }),
    setLanceTable: (t) => { lanceTable = t; },
    embed: (t: string) => embedBulk(t),
    minLength: MIN_EMBED_LENGTH,
    pageSize: EMBED_PAGE_SIZE,
  }));

  return {
    run: async () => {
      if (!lanceDb) {
        const r = await lanceInit();
        lanceDb = r.db;
        lanceTable = r.table;
      }
      if (!lanceDb) return { embedded: 0, skipped: 0, ollama_failures: 0 };
      return inner();
    },
  };
}
