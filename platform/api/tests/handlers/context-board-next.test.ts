/**
 * context-board-next handler tests (#2252).
 */

import {
  fetchContextBoardNext,
  type ContextBoardNextDeps,
} from '../../src/handlers/context-board-next';

function stubSparql(): ContextBoardNextDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

const PULSE_WITH_NEXT = JSON.stringify({
  board: {
    next_cards: [
      { id: 2251, owner: 'Kade', title: 'Memory endpoints', priority: 'P2', domain: 'chorus', status: 'Next' },
      { id: 2250, owner: 'Kade', title: 'Knowledge endpoints', priority: 'P2', domain: 'chorus', status: 'Next' },
      { id: 2400, owner: 'Wren', title: 'Wren thing', priority: 'P1', status: 'Next' },
    ],
  },
});

describe('fetchContextBoardNext', () => {
  it('returns next_cards sorted by id', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => PULSE_WITH_NEXT },
      '/api/chorus/context/board/next',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: Array<{ id: number }> } };
    expect(body.data.total).toBe(3);
    expect(body.data.cards[0].id).toBe(2250);
    expect(body.data.cards[2].id).toBe(2400);
  });

  it('filters by role when ?role given', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => PULSE_WITH_NEXT },
      '/api/chorus/context/board/next?role=kade',
      'kade',
    );
    const body = r.body as { data: { total: number; cards: Array<{ owner: string }> } };
    expect(body.data.total).toBe(2);
    expect(body.data.cards.every((c) => c.owner.toLowerCase() === 'kade')).toBe(true);
  });

  it('returns empty array when next_cards block absent', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => JSON.stringify({ board: {} }) },
      '/api/chorus/context/board/next',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: unknown[] } };
    expect(body.data.total).toBe(0);
    expect(body.data.cards).toEqual([]);
  });

  it('503 when pulse missing', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => null },
      '/api/chorus/context/board/next',
    );
    expect(r.status).toBe(503);
  });

  it('500 when pulse unparseable', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => 'not json' },
      '/api/chorus/context/board/next',
    );
    expect(r.status).toBe(500);
  });

  it('envelope carries source + domain=chorus + ISO timestamp', async () => {
    const r = await fetchContextBoardNext(
      { sparql: stubSparql(), readPulse: () => PULSE_WITH_NEXT },
      '/api/chorus/context/board/next',
    );
    const body = r.body as { source: string; domain?: string; timestamp: string };
    expect(body.source).toBe('/api/chorus/context/board/next');
    expect(body.domain).toBe('chorus');
    expect(body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T.*Z$/);
  });
});
