import { searchInTable, createLanceInit, maintainTable } from '../src/lance-store';

function fakeTable(rows: any[]) {
  return {
    vectorSearch: jest.fn((_vec: number[]) => ({
      limit: (_n: number) => ({
        toArray: async () => rows,
      }),
    })),
    countRows: async () => rows.length,
  };
}

describe('searchInTable', () => {
  it('returns [] when table is null', async () => {
    const embed = jest.fn();
    const out = await searchInTable(null, embed, 'q', 5);
    expect(out).toEqual([]);
    expect(embed).not.toHaveBeenCalled();
  });

  it('embeds the query and searches the table', async () => {
    const embed = jest.fn(async () => [0.1, 0.2]);
    const table = fakeTable([
      { msg_id: 1, source: 'slack', role: 'kade', content: 'hi', timestamp: 't', _distance: 0.1 },
    ]);
    const out = await searchInTable(table as any, embed, 'query', 10);
    expect(embed).toHaveBeenCalledWith('query');
    expect(table.vectorSearch).toHaveBeenCalledWith([0.1, 0.2]);
    expect(out).toHaveLength(1);
    expect(out[0].msg_id).toBe(1);
  });

  it('over-fetches then truncates to limit', async () => {
    const embed = jest.fn(async () => [0]);
    // vectorSearch.limit must receive limit*2 per the over-fetch contract.
    const limitSpy = jest.fn(() => ({ toArray: async () => [] }));
    const table = {
      vectorSearch: jest.fn(() => ({ limit: limitSpy })),
    };
    await searchInTable(table as any, embed, 'q', 7);
    expect(limitSpy).toHaveBeenCalledWith(14);
  });

  it('filters by role before slicing to limit', async () => {
    const embed = jest.fn(async () => [0]);
    const table = fakeTable([
      { msg_id: 1, role: 'kade', content: 'a', _distance: 0.1 },
      { msg_id: 2, role: 'wren', content: 'b', _distance: 0.1 },
      { msg_id: 3, role: 'kade', content: 'c', _distance: 0.1 },
    ]);
    const out = await searchInTable(table as any, embed, 'q', 5, 'kade');
    expect(out).toHaveLength(2);
    expect(out.every(r => r.role === 'kade')).toBe(true);
  });

  it('computes score as 1 / (1 + distance)', async () => {
    const embed = jest.fn(async () => [0]);
    const table = fakeTable([{ msg_id: 1, _distance: 0 }, { msg_id: 2, _distance: 1 }]);
    const out = await searchInTable(table as any, embed, 'q', 5);
    expect(out[0].score).toBe(1);           // 1 / (1+0)
    expect(out[1].score).toBe(0.5);          // 1 / (1+1)
  });

  it('uses 0 for score when _distance is missing', async () => {
    const embed = jest.fn(async () => [0]);
    const table = fakeTable([{ msg_id: 1 }]);
    const out = await searchInTable(table as any, embed, 'q', 5);
    expect(out[0].score).toBe(0);
  });

  it('coerces missing fields to empty strings', async () => {
    const embed = jest.fn(async () => [0]);
    const table = fakeTable([{ msg_id: 42, _distance: 0.5 }]);
    const out = await searchInTable(table as any, embed, 'q', 5);
    expect(out[0]).toEqual({
      msg_id: 42, source: '', channel: '', role: '', content: '', timestamp: '', score: 1 / 1.5,
    });
  });
});

describe('maintainTable (#3157 — compaction + version prune + re-index)', () => {
  const idxStub = (have: any[]) => ({
    listIndices: jest.fn(async () => have),
    createIndex: jest.fn(async () => {}),
  });

  it('optimizes with a cleanupOlderThan retention window', async () => {
    const optimize = jest.fn(async () => ({ compaction: { fragmentsRemoved: 31900 } }));
    const s = idxStub([{ columns: ['vector'], name: 'vector_idx' }]);
    const now = () => 1_000_000_000_000;
    const retentionMs = 24 * 60 * 60 * 1000;
    const r: any = await maintainTable({ optimize, ...s } as any, { retentionMs, now });
    const arg = (optimize.mock.calls[0] as any[])[0];
    expect(arg.cleanupOlderThan instanceof Date).toBe(true);
    expect(arg.cleanupOlderThan.getTime()).toBe(now() - retentionMs);
    expect(r.optimize).toEqual(expect.objectContaining({ compaction: expect.anything() }));
  });

  it('REBUILDS the vector index when compaction dropped it (listIndices empty)', async () => {
    const s = idxStub([]); // the live 2026-06-04 bug: compaction left no index
    const r: any = await maintainTable({ optimize: jest.fn(async () => ({})), ...s } as any, {});
    expect(s.createIndex).toHaveBeenCalledWith('vector', { replace: true });
    expect(r.reindexed).toBe(true);
  });

  it('skips re-index when the vector index already exists', async () => {
    const s = idxStub([{ columns: ['vector'], name: 'vector_idx' }]);
    const r: any = await maintainTable({ optimize: jest.fn(async () => ({})), ...s } as any, {});
    expect(s.createIndex).not.toHaveBeenCalled();
    expect(r.reindexed).toBe(false);
  });

  it('defaults to a 24h retention window when none given', async () => {
    const optimize = jest.fn(async () => ({}));
    const now = () => 2_000_000_000_000;
    await maintainTable({ optimize, ...idxStub([{ columns: ['vector'] }]) } as any, { now });
    const arg = (optimize.mock.calls[0] as any[])[0];
    expect(arg.cleanupOlderThan.getTime()).toBe(now() - 24 * 60 * 60 * 1000);
  });

  it('throws a clear error when the table has no optimize (old/wrong binding)', async () => {
    await expect(maintainTable({} as any)).rejects.toThrow(/optimize/i);
  });
});

describe('createLanceInit', () => {
  it('returns nullish state when LANCE_DIR does not exist', async () => {
    const fakeFs = { existsSync: jest.fn(() => false) };
    const connect = jest.fn();
    const init = createLanceInit({
      fs: fakeFs as any, lancedb: { connect } as any, lanceDir: '/nope',
    });
    const r = await init();
    expect(r.db).toBeNull();
    expect(r.table).toBeNull();
    expect(connect).not.toHaveBeenCalled();
  });

  it('opens the messages table when it exists', async () => {
    const fakeFs = { existsSync: jest.fn(() => true) };
    const openTable = jest.fn(async () => ({ countRows: async () => 42 }));
    const conn = { tableNames: async () => ['messages'], openTable };
    const connect = jest.fn(async () => conn);
    const init = createLanceInit({
      fs: fakeFs as any, lancedb: { connect } as any, lanceDir: '/ok',
    });
    const r = await init();
    expect(r.db).toBe(conn);
    expect(r.table).not.toBeNull();
    expect(openTable).toHaveBeenCalledWith('messages');
  });

  it('returns a connected db but null table when messages table is absent', async () => {
    const fakeFs = { existsSync: jest.fn(() => true) };
    const conn = { tableNames: async () => [], openTable: jest.fn() };
    const init = createLanceInit({
      fs: fakeFs as any,
      lancedb: { connect: async () => conn } as any,
      lanceDir: '/ok',
    });
    const r = await init();
    expect(r.db).toBe(conn);
    expect(r.table).toBeNull();
  });

  it('swallows connect errors and returns null db+table (non-fatal init)', async () => {
    const fakeFs = { existsSync: jest.fn(() => true) };
    const init = createLanceInit({
      fs: fakeFs as any,
      lancedb: { connect: async () => { throw new Error('lance down'); } } as any,
      lanceDir: '/boom',
    });
    const r = await init();
    expect(r.db).toBeNull();
    expect(r.table).toBeNull();
  });
});
