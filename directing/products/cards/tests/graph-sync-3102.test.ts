// @test-type: unit — pure row mapping and a fake graph door; no Vikunja, no athena-make.
// @card: #3102
// @owner: wren
/**
 * #3102 — every board card has a graph row that matches it, and one number
 * (out of sync) says how far the graph has drifted. Measured 2026-10-02: 474
 * rows for ~4,300 cards, written once on 09-27 and never again.
 */
import { cardRow, servedRow, rowDiff, reconcileGraph, syncCardToGraph, CardRow, GraphDoor } from '../src/graph-sync';
import { BoardTask } from '../src/types';

function task(over: Partial<BoardTask>): BoardTask {
  return {
    index: 1, apiId: 1, title: 't', description: '', status: 'Later', owner: 'Wren', priority: 'P2',
    domains: ['type:fix', 'domain:chorus'], done: false, created: '', updated: '', ...over,
  };
}

function fakeDoor(rows: Map<number, CardRow>, failOn = new Set<number>()): GraphDoor & { puts: number[] } {
  const puts: number[] = [];
  return {
    puts,
    async get(i) { return rows.get(i); },
    async list() { return new Map(rows); },
    async put(i, row) {
      if (failOn.has(i)) throw new Error('athena-make 503');
      puts.push(i);
      rows.set(i, row);
    },
  };
}

describe('#3102 the row a board card should have', () => {
  test('title, column, owner, priority and type map to the row', () => {
    expect(cardRow(task({ index: 4217, title: 'Retire v1', status: 'Keep', owner: 'Wren', priority: 'P3' })))
      .toEqual({ label: 'Retire v1', status: 'Keep', assignee: 'wren', priority: 'P3', cardType: 'fix' });
  });

  test('values outside the board vocabulary are left out, not invented', () => {
    expect(cardRow(task({ owner: '', priority: '', domains: ['domain:chorus'] })))
      .toEqual({ label: 't', status: 'Later' });
  });

  test('the served row reads back the same fields (role edge reduced to its name)', () => {
    expect(servedRow({ name: '4217', label: 'x', status: 'Keep', assignee: 'https://jeffbridwell.com/chorus#role-wren', priority: 'P3', cardType: 'fix', ownedBy: 'principal-wren' }))
      .toEqual({ label: 'x', status: 'Keep', assignee: 'wren', priority: 'P3', cardType: 'fix' });
  });
});

describe('#3102 the reconcile pass', () => {
  test('a retitled card is out of sync, gets repaired, and a second pass reads 0', async () => {
    const rows = new Map<number, CardRow>([[4217, { label: 'old title', status: 'Keep', assignee: 'wren', priority: 'P3', cardType: 'fix' }]]);
    const door = fakeDoor(rows);
    const board = [task({ index: 4217, title: 'new title', status: 'Keep', priority: 'P3' })];
    const first = await reconcileGraph(board, door);
    expect(first).toMatchObject({ boardCards: 1, outOfSync: 1, repaired: 1, failed: [] });
    expect(first.sample[0]).toEqual({ index: 4217, fields: ['label'] });
    const second = await reconcileGraph(board, door);
    expect(second.outOfSync).toBe(0);
  });

  test('a card with no row at all counts as missing and is created', async () => {
    const door = fakeDoor(new Map());
    const res = await reconcileGraph([task({ index: 9 })], door);
    expect(res.sample[0].fields).toEqual(['missing']);
    expect(door.puts).toEqual([9]);
  });

  test('--dry-run counts but writes nothing', async () => {
    const door = fakeDoor(new Map());
    const res = await reconcileGraph([task({ index: 9 })], door, { dryRun: true });
    expect(res.outOfSync).toBe(1);
    expect(door.puts).toEqual([]);
  });

  test('NEGATIVE PROOF: a board change made behind the cards tool shows up as out of sync', async () => {
    const rows = new Map<number, CardRow>([[5, cardRow(task({ index: 5 }))]]);
    expect((await reconcileGraph([task({ index: 5 })], fakeDoor(new Map(rows)), { dryRun: true })).outOfSync).toBe(0);
    // someone moves #5 to Done in the Vikunja web page
    const res = await reconcileGraph([task({ index: 5, status: 'Done' })], fakeDoor(new Map(rows)), { dryRun: true });
    expect(res.outOfSync).toBe(1);
    expect(res.sample[0].fields).toEqual(['status']);
  });

  test('NEGATIVE PROOF: a write that fails is counted as failed, never as repaired', async () => {
    const res = await reconcileGraph([task({ index: 7 })], fakeDoor(new Map(), new Set([7])));
    expect(res.repaired).toBe(0);
    expect(res.failed).toEqual([{ index: 7, reason: 'athena-make 503' }]);
  });

  test('rowDiff names exactly the fields that differ', () => {
    expect(rowDiff({ label: 'a', status: 'Next' }, { label: 'a', status: 'Next' })).toEqual([]);
    expect(rowDiff({ label: 'a', status: 'Next', priority: 'P1' }, { label: 'b', status: 'Next' })).toEqual(['label', 'priority']);
  });
});

describe('#3102 the write-through after a board write', () => {
  const saved = process.env.CARDS_GRAPH_SYNC;
  beforeEach(() => { delete process.env.CARDS_GRAPH_SYNC; });
  afterEach(() => { process.env.CARDS_GRAPH_SYNC = saved; });

  test('writes the card row through the door', async () => {
    const door = fakeDoor(new Map());
    expect(await syncCardToGraph(task({ index: 12, title: 'x' }), door)).toBe(true);
    expect(door.puts).toEqual([12]);
  });

  test('NEGATIVE PROOF: a failed graph write is reported on stderr and returns false, it does not throw', async () => {
    const writes: string[] = [];
    const spy = jest.spyOn(process.stderr, 'write').mockImplementation((s: string | Uint8Array) => { writes.push(String(s)); return true; });
    try {
      expect(await syncCardToGraph(task({ index: 13 }), fakeDoor(new Map(), new Set([13])))).toBe(false);
    } finally {
      spy.mockRestore();
    }
    expect(writes.join('')).toMatch(/card #13 changed on the board but its graph row was not updated: athena-make 503/);
  });
});
