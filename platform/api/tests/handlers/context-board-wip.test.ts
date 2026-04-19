/**
 * context-board-wip handler tests (#2234 Step 3).
 */

import {
  fetchContextBoardWip,
  type ContextBoardWipDeps,
} from '../../src/handlers/context-board-wip';

function stubSparql(): ContextBoardWipDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

const PULSE = JSON.stringify({
  board: {
    wip_cards: [
      { id: 2234, owner: 'Silas', title: 'Move chorus API', priority: 'P1', domain: 'chorus' },
      { id: 2241, owner: 'Kade', title: 'products/cards coverage', priority: 'P1', domain: 'chorus' },
      { id: 2218, owner: 'Silas', title: 'Codesign shim', priority: 'P1', domain: 'chorus' },
    ],
  },
});

describe('fetchContextBoardWip', () => {
  it('returns every WIP card sorted by id when no role filter', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => PULSE,
    }, '/api/chorus/context/board/wip');
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: { id: number }[] } };
    expect(body.data.total).toBe(3);
    expect(body.data.cards.map((c) => c.id)).toEqual([2218, 2234, 2241]);
  });

  it('filters to a single role case-insensitively', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => PULSE,
    }, '/api/chorus/context/board/wip?role=silas', 'silas');
    const body = r.body as { data: { total: number; cards: { owner: string }[] } };
    expect(body.data.total).toBe(2);
    body.data.cards.forEach((c) => expect(c.owner.toLowerCase()).toBe('silas'));
  });

  it('pulse snapshot missing → 503 with explanatory error', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => null,
    }, '/api/chorus/context/board/wip');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/no pulse snapshot/i);
  });

  it('unparseable pulse → 500 with error message', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => 'not json',
    }, '/api/chorus/context/board/wip');
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toMatch(/unparseable/);
  });

  it('pulse with no board section → empty cards list, 200', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => JSON.stringify({ health: { status: 'ok' } }),
    }, '/api/chorus/context/board/wip');
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: unknown[] } };
    expect(body.data.total).toBe(0);
    expect(body.data.cards).toEqual([]);
  });

  it('envelope is domain-scoped to chorus (step + product + domain present)', async () => {
    const sparql = {
      query: async () => ({
        results: {
          bindings: [{ step: { value: 'building' }, product: { value: 'chorus' } }],
        },
      }),
    };
    const r = await fetchContextBoardWip({
      sparql,
      readPulse: () => PULSE,
    }, '/api/chorus/context/board/wip');
    const body = r.body as { domain: string; step: string; product: string };
    expect(body.domain).toBe('chorus');
    expect(body.step).toBe('building');
    expect(body.product).toBe('chorus');
  });

  it('malformed card entries are shaped without throwing', async () => {
    const r = await fetchContextBoardWip({
      sparql: stubSparql(),
      readPulse: () => JSON.stringify({
        board: { wip_cards: [null, { id: 'not-a-number' }, { id: 42 }] },
      }),
    }, '/api/chorus/context/board/wip');
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: { id: number }[] } };
    // null filtered; { id: 'not-a-number' } becomes id=0; { id: 42 } kept.
    expect(body.data.total).toBe(2);
    expect(body.data.cards.map((c) => c.id).sort()).toEqual([0, 42]);
  });
});
