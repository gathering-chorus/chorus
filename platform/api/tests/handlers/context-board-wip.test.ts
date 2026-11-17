// @test-type: unit
// @domain: board — placed by chorus-crawl's domain rules (#4538)
// #4987 (AC1, AC2): the WIP list is read from the board itself, never a stale copy
/**
 * context-board-wip handler tests (#2234 Step 3; #4987).
 *
 * #4987 — the endpoint reads the board's own database at the moment of the question, not
 * pulse-latest.json. On 2026-11-17 that copy kept #4986 in WIP after its land, so the
 * pulse header said "reopened" and the Teammates tile said "building #4986" while the
 * board said Done. Both read this endpoint.
 */

import {
  fetchContextBoardWip,
  boardCardFromRow,
  BOARD_MAX_AGE_MS,
  type BoardCard,
  type ContextBoardWipDeps,
} from '../../src/handlers/context-board-wip';

function stubSparql(): ContextBoardWipDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

const NOW = new Date('2026-11-17T11:00:00Z');
const card = (id: string, owner: string, status: string, title = `card ${id}`): BoardCard =>
  ({ id, owner, status, title, priority: 'P1', domain: 'chorus' });

const BOARD = [
  card('2234', 'silas', 'WIP', 'Move chorus API'),
  card('2241', 'kade', 'WIP', 'products/cards coverage'),
  card('2218', 'silas', 'WIP', 'Codesign shim'),
  card('4986', 'wren', 'Done', 'Streams interleaved'),
  card('4988', 'abby-normal', 'Later'),
];

const deps = (cards: BoardCard[], readAt = NOW.getTime() - 30_000, sparql = stubSparql()): ContextBoardWipDeps =>
  ({ sparql, readBoard: () => ({ cards, readAt }), now: () => NOW });

const ids = (r: Awaited<ReturnType<typeof fetchContextBoardWip>>) =>
  (r.body as { data: { cards: { id: number }[] } }).data.cards.map((c) => c.id);

describe('fetchContextBoardWip', () => {
  it('returns every WIP card on the board, sorted by id, and nothing else', async () => {
    const r = await fetchContextBoardWip(deps(BOARD), '/api/chorus/context/board/wip');
    expect(r.status).toBe(200);
    expect((r.body as { data: { total: number } }).data.total).toBe(3);
    expect(ids(r)).toEqual([2218, 2234, 2241]);
    expect((r.body as { data: { cards: { priority: string }[] } }).data.cards[0].priority).toBe('P1');
  });

  it('NEGATIVE PROOF (#4987 AC1/AC2): a card that landed and is Done on the board is not in WIP, whatever a copy said', async () => {
    const r = await fetchContextBoardWip(deps(BOARD), '/api/chorus/context/board/wip');
    expect(ids(r)).not.toContain(4986);
    // and a card the board does hold in WIP still shows: the filter is the board's status, not "drop everything"
    const reopened = await fetchContextBoardWip(deps([...BOARD.filter((c) => c.id !== '4986'), card('4986', 'wren', 'WIP')]), '/x');
    expect(ids(reopened)).toContain(4986);
  });

  it('filters to a single role case-insensitively, any owner name', async () => {
    const r = await fetchContextBoardWip(deps([...BOARD, card('4990', 'abby-normal', 'WIP')]), '/x', 'Abby-Normal');
    expect(ids(r)).toEqual([4990]);
    const silas = await fetchContextBoardWip(deps(BOARD), '/x', 'SILAS');
    expect(ids(silas)).toEqual([2218, 2234]);
  });

  it('NEGATIVE PROOF: a board never read is unreadable (503), never an empty list', async () => {
    const r = await fetchContextBoardWip(deps([], 0), '/x');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/not read yet/);
  });

  it('NEGATIVE PROOF: a board read older than the limit is unreadable (503), never shown as now', async () => {
    const r = await fetchContextBoardWip(deps(BOARD, NOW.getTime() - BOARD_MAX_AGE_MS - 1000), '/x');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/last read \d+ s ago/);
    const fresh = await fetchContextBoardWip(deps(BOARD, NOW.getTime() - BOARD_MAX_AGE_MS + 1000), '/x');
    expect(fresh.status).toBe(200);
  });

  it('NEGATIVE PROOF: a board that cannot be read is unreadable (503) with the reason, never an empty list', async () => {
    const r = await fetchContextBoardWip({ sparql: stubSparql(), readBoard: () => { throw new Error('unable to open database file'); }, now: () => NOW }, '/x');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toBe('board unreadable: unable to open database file');
  });

  it('a board row takes owner, priority and domain from the board\'s own labels', () => {
    expect(boardCardFromRow({ id: 4987, title: 'T', status: 'WIP', labels: 'owner:wren|P2|domain:chorus|sequence:chorus|type:fix' }))
      .toEqual({ id: '4987', title: 'T', status: 'WIP', owner: 'wren', priority: 'P2', domain: 'chorus' });
    expect(boardCardFromRow({ id: 1, title: 'T', status: 'WIP', labels: 'owner:Abby-normal' }).owner).toBe('abby-normal');
    expect(boardCardFromRow({ id: 2, title: 'T', status: 'WIP', labels: '' })).toMatchObject({ owner: '', priority: '', domain: '' });
  });

  it('a real empty WIP column answers 200 with total 0', async () => {
    const r = await fetchContextBoardWip(deps([card('1', 'wren', 'Done')]), '/x');
    expect(r.status).toBe(200);
    expect((r.body as { data: { total: number } }).data.total).toBe(0);
  });

  it('envelope is domain-scoped to chorus (step + product + domain present)', async () => {
    const sparql = {
      query: async () => ({
        results: {
          bindings: [{ step: { value: 'building' }, product: { value: 'chorus' } }],
        },
      }),
    };
    const r = await fetchContextBoardWip(deps(BOARD, NOW.getTime() - 1000, sparql), '/api/chorus/context/board/wip');
    const body = r.body as { domain: string; step: string; product: string };
    expect(body.domain).toBe('chorus');
    expect(body.step).toBe('building');
  });
});
