// #3077 AC3: embed must not run a COUNT(*) full-scan (1.26M rows) on every call —
// the COUNT existed only to decorate a log line. And the page query WHERE embedded=0
// must be index-backed so the query stops scanning past mostly-embedded rows.
// Restructure, not worker-thread (the UPDATEs are already batched in a transaction).
import { createEmbedDelta } from '../src/embed-delta';

function recordingDbFactory(messages: any[]) {
  const prepared: string[] = [];
  const execed: string[] = [];
  const state = { marked: new Set<number>() };
  const ctor = jest.fn((_p: string, _o?: any) => ({
    pragma: jest.fn(),
    exec: (sql: string) => { execed.push(sql); },
    prepare: (sql: string) => {
      prepared.push(sql);
      if (sql.includes('FROM messages') && sql.includes('LIMIT')) {
        return { all: (_min: number, limit: number) => messages.slice(0, limit), get: () => undefined };
      }
      if (sql.includes('UPDATE messages SET embedded = 1')) {
        return { run: (id: number) => { state.marked.add(id); } };
      }
      return { all: () => [], get: () => undefined, run: () => {} };
    },
    transaction: (fn: (ids: number[]) => void) => (ids: number[]) => fn(ids),
    close: jest.fn(),
  }));
  return { ctor, prepared, execed, state };
}

const deps = (ctor: any) => ({
  dbPath: '/db', DatabaseCtor: ctor,
  getLanceStore: () => ({ db: null, table: { add: jest.fn() } as any }),
  setLanceTable: jest.fn(),
  embed: jest.fn(async () => [0.1, 0.2]),
  minLength: 100, pageSize: 10, log: jest.fn(), error: jest.fn(),
});

describe('#3077 AC3 — embed: drop the COUNT(*) full-scan + index the embedded column', () => {
  it('runs no SELECT COUNT(*) full-scan', async () => {
    const { ctor, prepared } = recordingDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'x'.repeat(200), timestamp: 't' },
    ]);
    await createEmbedDelta(deps(ctor) as any)();
    const countQueries = prepared.filter(s => s.toUpperCase().includes('COUNT(*)'));
    expect(countQueries).toHaveLength(0); // the 1.26M-row full-scan is gone
  });

  it('creates an index on the embedded column so the page query avoids a full scan', async () => {
    const { ctor, execed } = recordingDbFactory([]);
    await createEmbedDelta(deps(ctor) as any)();
    const idx = execed.filter(s => s.toUpperCase().includes('CREATE INDEX') && s.includes('embedded'));
    expect(idx.length).toBeGreaterThanOrEqual(1);
  });

  it('still embeds and marks the page (behavior preserved)', async () => {
    const { ctor, state } = recordingDbFactory([
      { id: 1, source: 's', channel: 'c', role: 'r', content: 'a'.repeat(200), timestamp: 't' },
      { id: 2, source: 's', channel: 'c', role: 'r', content: 'b'.repeat(200), timestamp: 't' },
    ]);
    const r = await createEmbedDelta(deps(ctor) as any)();
    expect(r.embedded).toBe(2);
    expect(state.marked.has(1)).toBe(true);
    expect(state.marked.has(2)).toBe(true);
  });
});
