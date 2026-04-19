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

export type VectorTable = {
  vectorSearch: (vec: number[]) => { limit: (n: number) => { toArray: () => Promise<any[]> } };
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
    filtered = results.filter((r: any) => r.role === role);
  }

  return filtered.slice(0, limit).map((r: any) => ({
    msg_id: r.msg_id,
    source: r.source || '',
    channel: r.channel || '',
    role: r.role || '',
    content: r.content || '',
    timestamp: r.timestamp || '',
    score: r._distance != null ? 1 / (1 + r._distance) : 0,
  }));
}

export interface LanceInitDeps {
  fs: { existsSync: (p: string) => boolean };
  lancedb: { connect: (dir: string) => Promise<any> };
  lanceDir: string;
  logger?: { log?: (m: string) => void; error?: (m: string) => void };
}

export interface LanceInitResult {
  db: any | null;
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
        const count = await table.countRows();
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
