/**
 * #3382 — pure message logic for the off-process semantic-search worker. Kept
 * separate from the process glue (search-worker.ts) so it's unit-testable
 * without spawning. Runs the SAME searchInTable the in-process path used →
 * parity by construction; the only change is WHERE it runs (a separate process,
 * so lance's native CPU pool never starves chorus-api's event loop).
 */
import { searchInTable, type VectorTable, type Embedder } from './lance-store';

export interface SearchRequest {
  id: number;
  query: string;
  limit: number;
  role?: string;
  /** #3606 — 'count' asks for the table's row count (the health cache's vector
   *  stat; the in-process lance handle was removed by #3382 and its stub made
   *  /health/detail report vectors:0 forever). Default: 'search'. */
  op?: 'search' | 'count';
}

export type SearchReply = { id: number; rows: unknown[] } | { id: number; error: string };

export interface SearchDeps {
  table: VectorTable | null;
  embed: Embedder;
}

/**
 * Run one semantic-search request against the worker's lance handle + embedder.
 * Any failure (embed error, lance error) becomes an error reply rather than a
 * throw, so a bad message can never crash the worker (the pool still guards
 * process-level crash + timeout separately).
 */
export async function handleSearchMessage(deps: SearchDeps, msg: SearchRequest): Promise<SearchReply> {
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- msg arrives via worker IPC; typed but may be malformed, keep the guard
  const id = msg && typeof msg.id === 'number' ? msg.id : -1;
  try {
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- IPC message; op may be absent on old callers
    if (msg && msg.op === 'count') {
      const count = (await deps.table?.countRows?.()) ?? 0;
      return { id, rows: [{ count }] };
    }
    const rows = await searchInTable(deps.table, deps.embed, msg.query, msg.limit, msg.role);
    return { id, rows };
  } catch (e) {
    return { id, error: e instanceof Error ? e.message : String(e) };
  }
}
