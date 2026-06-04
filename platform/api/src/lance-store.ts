// LanceDB connection + semantic search helpers (extracted from server.ts
// for #2205 wave 11).
//
// Two exports:
// - createLanceInit: boot-path factory that opens the messages table.
// - searchInTable: pure-ish vector search against an already-opened table,
//   takes an embedder so tests can stub Ollama out.
//
// Module state stays in server.ts (lanceTable/lanceDb are still mutated
// by embedDelta on first table creation). This wave is about extracting
// the testable seams, not relocating ownership.

import type { SemanticResult } from './search-fusion';

/** Row shape returned by LanceDB vectorSearch — subset used here. */
export interface LanceRow {
  msg_id?: number;
  source?: string;
  channel?: string;
  role?: string;
  content?: string;
  timestamp?: string;
  _distance?: number;
}

export type VectorTable = {
  vectorSearch: (vec: number[]) => { limit: (n: number) => { toArray: () => Promise<LanceRow[]> } };
  countRows?: () => Promise<number>;
};

export type Embedder = (text: string) => Promise<number[]>;

/**
 * Vector-search an opened Lance table by embedding the query, over-fetching
 * by 2x, optionally filtering by role, then slicing to the caller's limit
 * and normalizing each hit into SemanticResult shape.
 */
export async function searchInTable(
  table: VectorTable | null,
  embed: Embedder,
  query: string,
  limit: number,
  role?: string,
): Promise<SemanticResult[]> {
  if (!table) return [];
  const queryVec = await embed(query);
  const builder = table.vectorSearch(queryVec).limit(limit * 2);
  const results = await builder.toArray();

  let filtered = results;
  if (role) {
    filtered = results.filter((r) => r.role === role);
  }

  return filtered.slice(0, limit).map((r) => ({
    msg_id: r.msg_id ?? 0,
    source: r.source || '',
    channel: r.channel || '',
    role: r.role || '',
    content: r.content || '',
    timestamp: r.timestamp || '',
    score: r._distance != null ? 1 / (1 + r._distance) : 0,
  }));
}

/** Table surface needed for maintenance — lance's optimize + index ops. */
export type MaintainableTable = {
  optimize: (opts?: { cleanupOlderThan?: Date; deleteUnverified?: boolean }) => Promise<unknown>;
  listIndices?: () => Promise<Array<{ columns?: string[]; name?: string }>>;
  createIndex?: (column: string, opts?: { replace?: boolean }) => Promise<void>;
};

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * #3157 — compact fragments + prune old versions via lance's own optimize().
 *
 * messages.lance grew to 31G — 24G of uncompacted version manifests (10.8K
 * versions) + 31.9K fragment files — because nothing ever compacted it (every
 * write appends a fragment + a version, nothing merges/expires). optimize()
 * merges fragments AND, with `cleanupOlderThan`, prunes versions older than the
 * retention window. This is lance's sanctioned, atomic maintenance — NOT a
 * hand-rolled file delete (the index.db-cp-corruption class we must never repeat).
 * The retention window keeps recent versions so in-flight readers stay safe.
 *
 * BUT compaction also INVALIDATES the vector index — proven live 2026-06-04:
 * running optimize dropped it (listIndices()=[]) and semantic search began
 * timing out (brute-force over 2M vectors). So after optimize we re-ensure the
 * vector index exists, or scheduled compaction silently re-breaks semantic
 * search every run. (The missing index was also the original "pegs a core".)
 *
 * Returns { optimize: <lance stats>, reindexed } so the caller logs the reclaim
 * and whether the index had to be rebuilt.
 */
export async function maintainTable(
  table: MaintainableTable,
  opts: { retentionMs?: number; vectorColumn?: string; now?: () => number } = {},
): Promise<{ optimize: unknown; reindexed: boolean }> {
  if (typeof table?.optimize !== 'function') {
    throw new Error('maintainTable: table has no optimize() — lance binding too old or wrong surface');
  }
  const { retentionMs = DAY_MS, vectorColumn = 'vector', now = Date.now } = opts;
  const cleanupOlderThan = new Date(now() - retentionMs);
  const optimizeStats = await table.optimize({ cleanupOlderThan });

  let reindexed = false;
  if (typeof table.listIndices === 'function' && typeof table.createIndex === 'function') {
    const indices = (await table.listIndices()) ?? [];
    const hasVectorIndex = indices.some((i) => (i.columns ?? []).includes(vectorColumn));
    if (!hasVectorIndex) {
      await table.createIndex(vectorColumn, { replace: true });
      reindexed = true;
    }
  }
  return { optimize: optimizeStats, reindexed };
}

/** Minimal LanceDB connection surface used here. */
export interface LanceDbConnection {
  openTable: (name: string) => Promise<VectorTable>;
  tableNames: () => Promise<string[]>;
}

export interface LanceInitDeps {
  fs: { existsSync: (p: string) => boolean };
  lancedb: { connect: (dir: string) => Promise<LanceDbConnection> };
  lanceDir: string;
  logger?: { log?: (m: string) => void; error?: (m: string) => void };
}

export interface LanceInitResult {
  db: LanceDbConnection | null;
  table: VectorTable | null;
}

/**
 * Boot-path initializer. Returns a { db, table } pair or nulls if the
 * Lance dir is missing, connect fails, or the messages table is absent.
 * Never throws — init failures are non-fatal for the API.
 */
export function createLanceInit(deps: LanceInitDeps): () => Promise<LanceInitResult> {
  const log = deps.logger?.log ?? ((m: string) => console.log(m));
  const err = deps.logger?.error ?? ((m: string) => console.error(m));
  return async (): Promise<LanceInitResult> => {
    if (!deps.fs.existsSync(deps.lanceDir)) return { db: null, table: null };
    try {
      const db = await deps.lancedb.connect(deps.lanceDir);
      const tables = await db.tableNames();
      if (tables.includes('messages')) {
        const table = await db.openTable('messages');
        const count = await table.countRows?.() ?? 0;
        log(`[chorus-api] LanceDB: ${count} vectors loaded`);
        return { db, table };
      }
      return { db, table: null };
    } catch (e) {
      err(`[chorus-api] LanceDB init failed (non-fatal): ${e}`);
      return { db: null, table: null };
    }
  };
}
