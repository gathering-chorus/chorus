// @test-type: unit — fake lance table + fake embedder; no process spawn, no live services.
/**
 * #3382 — pure message logic for the off-process semantic-search worker.
 * Tests the handler in isolation (fake lance table + fake embedder), no real
 * process, so the worker's reply contract is verified without spawning.
 */
import { handleSearchMessage, type SearchRequest } from '../src/search-worker-core';

// Minimal fake of the lance VectorTable surface searchInTable consumes.
function fakeTable(rows: Array<{ msg_id: number; role?: string; _distance?: number }>) {
  return {
    vectorSearch: (_vec: number[]) => ({
      limit: (_n: number) => ({ toArray: async () => rows }),
    }),
  };
}
const fakeEmbed = async (_q: string) => [0.1, 0.2, 0.3];

describe('handleSearchMessage (#3382)', () => {
  it('returns {id, rows} with normalized semantic results', async () => {
    const table = fakeTable([{ msg_id: 7, role: 'wren', _distance: 0 }]);
    const msg: SearchRequest = { id: 5, query: 'hello', limit: 10 };
    const reply = await handleSearchMessage({ table, embed: fakeEmbed }, msg);
    expect(reply.id).toBe(5);
    expect('rows' in reply).toBe(true);
    if (!('rows' in reply)) throw new Error('expected rows'); // narrow; unreachable past the assert
    expect(reply.rows.length).toBe(1);
    expect((reply.rows[0] as { msg_id: number }).msg_id).toBe(7);
    expect((reply.rows[0] as { score: number }).score).toBe(1); // 1/(1+0)
  });

  it('filters by role when role is given', async () => {
    const table = fakeTable([
      { msg_id: 1, role: 'wren' },
      { msg_id: 2, role: 'silas' },
    ]);
    const reply = await handleSearchMessage(
      { table, embed: fakeEmbed },
      { id: 9, query: 'q', limit: 10, role: 'silas' },
    );
    if (!('rows' in reply)) throw new Error('expected rows');
    expect(reply.rows.length).toBe(1);
    expect((reply.rows[0] as { msg_id: number }).msg_id).toBe(2);
  });

  it('a null table returns empty rows, never throws', async () => {
    const reply = await handleSearchMessage({ table: null, embed: fakeEmbed }, { id: 3, query: 'q', limit: 5 });
    expect(reply).toEqual({ id: 3, rows: [] });
  });

  it('an embedder failure becomes {id, error}, never a throw', async () => {
    const table = fakeTable([{ msg_id: 1 }]);
    const boom = async () => { throw new Error('ollama down'); };
    const reply = await handleSearchMessage({ table, embed: boom }, { id: 11, query: 'q', limit: 5 });
    expect(reply).toEqual({ id: 11, error: 'ollama down' });
  });

  it('preserves a non-numeric id as -1 rather than crashing', async () => {
    const table = fakeTable([]);
    // deliberately malformed message
    const reply = await handleSearchMessage(
      { table, embed: fakeEmbed },
      { query: 'q', limit: 5 } as unknown as SearchRequest,
    );
    expect(reply.id).toBe(-1);
  });

  // #3606 — the health cache's vector count. When lance moved off-process
  // (#3382), server.ts stubbed getLanceTable() → null and /health/detail
  // reported vectors:0 forever — test-api-health red + lancedb-stale alert
  // false-firing while 13GB of vectors sat on disk. The count now rides the
  // worker protocol as an op:'count' request.
  it("op:'count' replies with the table's row count, no embed involved", async () => {
    const table = { ...fakeTable([]), countRows: async () => 33417 };
    const neverEmbed = async () => { throw new Error('embed must not be called for count'); };
    const reply = await handleSearchMessage(
      { table, embed: neverEmbed },
      { id: 21, query: '', limit: 0, op: 'count' },
    );
    if (!('rows' in reply)) throw new Error('expected rows');
    expect(reply.rows).toEqual([{ count: 33417 }]);
  });

  it("op:'count' with no table replies count 0, not an error", async () => {
    const reply = await handleSearchMessage(
      { table: null, embed: fakeEmbed },
      { id: 22, query: '', limit: 0, op: 'count' },
    );
    expect(reply).toEqual({ id: 22, rows: [{ count: 0 }] });
  });
});
