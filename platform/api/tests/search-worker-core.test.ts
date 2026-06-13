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
    if ('rows' in reply) {
      expect(reply.rows.length).toBe(1);
      expect((reply.rows[0] as { msg_id: number }).msg_id).toBe(7);
      expect((reply.rows[0] as { score: number }).score).toBe(1); // 1/(1+0)
    }
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
    if ('rows' in reply) {
      expect(reply.rows.length).toBe(1);
      expect((reply.rows[0] as { msg_id: number }).msg_id).toBe(2);
    } else {
      throw new Error('expected rows');
    }
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
});
