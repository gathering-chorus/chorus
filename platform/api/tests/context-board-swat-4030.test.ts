// @test-type: unit — injects pulse + sparql stubs; no Fuseki, no live service, brings its own world.
// #4030 — GET /api/chorus/context/board/swat (#2261). Jeff's experience under
// test: "what is in the SWAT lane right now" answers from the pulse snapshot,
// per role when asked, smallest card first, and says so honestly when there is
// no snapshot to read.
import { fetchContextBoardSwat } from '../src/handlers/context-board-swat';

const sparql = { query: async () => ({ results: { bindings: [] } }) };
const URL = 'http://localhost:3340/api/chorus/context/board/swat';

function pulse(swat: unknown): string {
  return JSON.stringify({ board: { swat_cards: swat } });
}

describe('context/board/swat', () => {
  it('lists the SWAT cards from pulse, smallest id first, with an envelope', async () => {
    const r = await fetchContextBoardSwat(
      { sparql, readPulse: () => pulse([
        { id: 3020, owner: 'Kade', title: 'B', priority: 'P1', domain: 'chorus' },
        { id: 2999, owner: 'Silas', title: 'A', priority: 'P1' },
      ]) },
      URL,
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; cards: Array<{ id: number; domain?: string }> } };
    expect(body.data.total).toBe(2);
    expect(body.data.cards.map((c) => c.id)).toEqual([2999, 3020]);
    expect(body.data.cards[1].domain).toBe('chorus');
    expect(body.data.cards[0].domain).toBeUndefined();
  });

  it('?role= filters to that role, case-insensitively', async () => {
    const r = await fetchContextBoardSwat(
      { sparql, readPulse: () => pulse([
        { id: 1, owner: 'Kade', title: 'k', priority: 'P2' },
        { id: 2, owner: 'Silas', title: 's', priority: 'P2' },
      ]) },
      URL,
      'silas',
    );
    const body = r.body as { data: { total: number; cards: Array<{ owner: string }> } };
    expect(body.data.total).toBe(1);
    expect(body.data.cards[0].owner).toBe('Silas');
  });

  it('no pulse snapshot → 503 that says the board state is unknown, never an empty 200', async () => {
    const r = await fetchContextBoardSwat({ sparql, readPulse: () => null }, URL);
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/unknown/);
  });

  it('an unparseable snapshot → 500 naming the parse error', async () => {
    const r = await fetchContextBoardSwat({ sparql, readPulse: () => '{not json' }, URL);
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toMatch(/unparseable/);
  });

  it('a snapshot without a swat lane, or with junk rows, reads as an empty lane', async () => {
    const none = await fetchContextBoardSwat({ sparql, readPulse: () => JSON.stringify({ board: {} }) }, URL);
    expect((none.body as { data: { total: number } }).data.total).toBe(0);
    const junk = await fetchContextBoardSwat({ sparql, readPulse: () => pulse([null, 'x', { id: 'nope' }]) }, URL);
    const body = junk.body as { data: { total: number; cards: Array<{ id: number; owner: string }> } };
    expect(body.data.total).toBe(1);
    expect(body.data.cards[0]).toEqual({ id: 0, owner: '', title: '', priority: '' });
  });
});
