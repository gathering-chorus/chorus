import { createEmbedDelta, singleFlight } from '../src/embed-delta';

describe('singleFlight (#3214 — coalesce concurrent embedDelta runs)', () => {
  it('coalesces concurrent calls into ONE in-flight run', async () => {
    let resolve!: () => void;
    const gate = new Promise<void>((r) => { resolve = r; });
    let calls = 0;
    const fn = jest.fn(async () => { calls += 1; await gate; return calls; });
    const wrapped = singleFlight(fn);
    const a = wrapped(); const b = wrapped(); const c = wrapped();
    expect(fn).toHaveBeenCalledTimes(1); // 3 concurrent triggers -> 1 backfill loop
    resolve();
    const [ra, rb, rc] = await Promise.all([a, b, c]);
    expect([ra, rb, rc]).toEqual([1, 1, 1]); // all share the one run's result
  });

  it('starts a fresh run after the previous settles', async () => {
    let calls = 0;
    const fn = jest.fn(async () => ++calls);
    const wrapped = singleFlight(fn);
    expect(await wrapped()).toBe(1);
    expect(await wrapped()).toBe(2);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('clears in-flight on rejection so the next call retries (no stuck promise)', async () => {
    let calls = 0;
    const fn = jest.fn(async () => { calls += 1; if (calls === 1) throw new Error('boom'); return calls; });
    const wrapped = singleFlight(fn);
    await expect(wrapped()).rejects.toThrow('boom');
    expect(await wrapped()).toBe(2);
  });
});

// In-memory fake better-sqlite3 surface: exec/prepare(all|get|run)/transaction/close.
function fakeDbFactory(initialMessages: any[]) {
  const state = {
    messages: [...initialMessages],
    markedIds: new Set<number>(),
    schemaApplied: false,
    closedCount: 0,
  };
  const ctor = jest.fn((_path: string, _opts?: any) => ({
    pragma: jest.fn(),
    exec: (sql: string) => {
      if (sql.includes('ADD COLUMN embedded')) {
        state.schemaApplied = true;
      }
    },
    prepare: (sql: string) => {
      if (sql.includes('SELECT COUNT(*)')) {
        return { get: () => ({ cnt: state.messages.length }), all: () => [] };
      }
      if (sql.includes('FROM messages')) {
        return {
          all: (_min: number, limit: number) => state.messages.slice(0, limit),
          get: () => ({ cnt: state.messages.length }),
        };
      }
      if (sql.includes('UPDATE messages SET embedded = 1')) {
        return {
          run: (id: number) => { state.markedIds.add(id); },
        };
      }
      return { all: () => [], get: () => ({ cnt: 0 }), run: () => {} };
    },
    transaction: (fn: (ids: number[]) => void) => (ids: number[]) => fn(ids),
    close: () => { state.closedCount++; },
  }));
  return { ctor, state };
}

describe('createEmbedDelta', () => {
  it('returns zero stats when the page is empty', async () => {
    const { ctor } = fakeDbFactory([]);
    const lanceTable = { add: jest.fn() };
    const embed = jest.fn();
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: lanceTable as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    const r = await run();
    expect(r).toEqual({ embedded: 0, skipped: 0, ollama_failures: 0 });
    expect(embed).not.toHaveBeenCalled();
    expect(lanceTable.add).not.toHaveBeenCalled();
  });

  it('adds records to an existing lance table when embedding succeeds', async () => {
    const { ctor, state } = fakeDbFactory([
      { id: 1, source: 'slack', channel: 'c', role: 'kade', content: 'hello world', timestamp: 't' },
      { id: 2, source: 'slack', channel: 'c', role: 'wren', content: 'another message', timestamp: 't' },
    ]);
    const lanceTable = { add: jest.fn() };
    const embed = jest.fn(async (_text: string) => [0.1, 0.2, 0.3]);
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: lanceTable as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    const r = await run();
    expect(r.embedded).toBe(2);
    expect(lanceTable.add).toHaveBeenCalledTimes(1);
    expect(state.markedIds.has(1)).toBe(true);
    expect(state.markedIds.has(2)).toBe(true);
  });

  it('creates the lance table on first use when only lance db is available', async () => {
    const { ctor } = fakeDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'x', timestamp: 't' },
    ]);
    const createdTable = { add: jest.fn() };
    const lanceDb = { createTable: jest.fn(async () => createdTable) };
    const setLanceTable = jest.fn();
    const embed = jest.fn(async () => [0.1]);
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: lanceDb as any, table: null }),
      setLanceTable,
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    const r = await run();
    expect(lanceDb.createTable).toHaveBeenCalledWith('messages', expect.any(Array));
    expect(setLanceTable).toHaveBeenCalledWith(createdTable);
    expect(r.embedded).toBe(1);
  });

  it('counts ollama failures as skipped and reports them', async () => {
    const { ctor } = fakeDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'a'.repeat(100), timestamp: 't' },
      { id: 2, source: 's', channel: 'c', role: 'r', content: 'b'.repeat(100), timestamp: 't' },
    ]);
    let call = 0;
    const embed = jest.fn(async () => {
      call++;
      if (call === 1) throw new Error('ollama down');
      return [0.1];
    });
    const lanceTable = { add: jest.fn() };
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: lanceTable as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    const r = await run();
    expect(r.embedded).toBe(1);
    expect(r.skipped).toBe(1);
    expect(r.ollama_failures).toBe(1);
  });

  it('returns zero embedded when every message fails ollama', async () => {
    const { ctor } = fakeDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'x', timestamp: 't' },
    ]);
    const embed = jest.fn(async () => { throw new Error('all fail'); });
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: { add: jest.fn() } as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    const r = await run();
    expect(r.embedded).toBe(0);
    expect(r.skipped).toBe(1);
    expect(r.ollama_failures).toBe(1);
  });

  it('truncates content to 2000 chars in both the embed call and stored record', async () => {
    const longContent = 'x'.repeat(5000);
    const { ctor } = fakeDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: longContent, timestamp: 't' },
    ]);
    const embed = jest.fn(async (text: string) => {
      expect(text.length).toBeLessThanOrEqual(2100); // wrapper prefix + 2000
      return [0];
    });
    const lanceTable = { add: jest.fn() };
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: lanceTable as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
    });
    await run();
    const records = lanceTable.add.mock.calls[0][0] as any[];
    expect(records[0].content.length).toBe(2000);
  });

  it('passes minLength as the WHERE parameter to SQL and PAGE_SIZE as limit', async () => {
    const capture: any = { min: null, limit: null };
    const ctor = jest.fn((_p: string, _o?: any) => ({
      pragma: jest.fn(), exec: jest.fn(),
      prepare: (sql: string) => ({
        all: (min: number, limit: number) => { capture.min = min; capture.limit = limit; return []; },
        get: () => ({ cnt: 0 }),
        run: () => {},
      }),
      transaction: (fn: any) => fn,
      close: () => {},
    }));
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: null }),
      setLanceTable: jest.fn(),
      embed: jest.fn(), minLength: 250, pageSize: 42, log: jest.fn(), error: jest.fn(),
    });
    await run();
    expect(capture.min).toBe(250);
    expect(capture.limit).toBe(42);
  });

  it('logs success and the failure counter when ollama had failures', async () => {
    const { ctor } = fakeDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'a', timestamp: 't' },
      { id: 2, source: 's', channel: 'c', role: 'r', content: 'b', timestamp: 't' },
    ]);
    let call = 0;
    const embed = jest.fn(async () => {
      call++;
      if (call === 1) throw new Error('fail');
      return [0];
    });
    const log = jest.fn();
    const run = createEmbedDelta({
      dbPath: '/db', DatabaseCtor: ctor as any,
      getLanceStore: () => ({ db: null, table: { add: jest.fn() } as any }),
      setLanceTable: jest.fn(),
      embed, minLength: 100, pageSize: 10, log, error: jest.fn(),
    });
    await run();
    expect(log).toHaveBeenCalledWith(expect.stringContaining('ollama_failures 1'));
  });
});
