/**
 * #2754 — embed floor behavior test.
 *
 * Pre-fix: MIN_EMBED_LENGTH=100 silently filtered short messages out of the
 * embedding pass. Jeff's communication style is short imperatives ("yes",
 * "do it", "make the card") — a 100-char floor erased him from semantic
 * recall (Jeff: 905,583 total / 772,190 embedded → 133,391 sub-100-char
 * messages skipped, including most of Jeff's input).
 *
 * The fix exports MIN_EMBED_LENGTH from server.ts and lowers it to 1.
 * This test wires the real production constant through createEmbedDelta
 * against a fake DB that honors LENGTH(content) >= minLength, and verifies
 * that a 3-char Jeff-shaped message ("yes") flows through to the embedder.
 *
 * Will be red while MIN_EMBED_LENGTH=100, green at 1.
 */
import { createEmbedDelta } from '../src/embed-delta';
import { MIN_EMBED_LENGTH } from '../src/embed-floor';

type FakeMsg = {
  id: number;
  source: string;
  channel: string;
  role: string;
  content: string;
  timestamp: string;
};

function filteringDbFactory(messages: FakeMsg[]) {
  const state = { messages: [...messages], markedIds: new Set<number>() };
  const ctor = jest.fn((_path: string, _opts?: unknown) => ({
    pragma: jest.fn(),
    exec: jest.fn(),
    prepare: (sql: string) => {
      if (sql.includes('UPDATE messages SET embedded = 1')) {
        return { run: (id: number) => { state.markedIds.add(id); } };
      }
      if (sql.includes('SELECT COUNT(*)')) {
        return {
          get: (min: number) =>
            ({ cnt: state.messages.filter((m) => m.content.length >= min).length }),
          all: () => [],
        };
      }
      if (sql.includes('FROM messages')) {
        return {
          all: (min: number, limit: number) =>
            state.messages
              .filter((m) => m.content.length >= min)
              .slice(0, limit),
          get: () => ({ cnt: 0 }),
        };
      }
      return { all: () => [], get: () => ({ cnt: 0 }), run: () => {} };
    },
    transaction: (fn: (ids: number[]) => void) => (ids: number[]) => fn(ids),
    close: jest.fn(),
  }));
  return { ctor, state };
}

describe('#2754 embed floor — short Jeff-shaped messages get embedded', () => {
  test('production MIN_EMBED_LENGTH lets a 3-char message through to the embedder', async () => {
    const jeffMessage: FakeMsg = {
      id: 42, source: 'claude', channel: 'session:silas', role: 'jeff',
      content: 'yes', timestamp: '2026-05-06T18:30:00Z',
    };
    const { ctor, state } = filteringDbFactory([jeffMessage]);
    const lanceTable = { add: jest.fn() };
    const embed = jest.fn(async (_t: string) => [0.1, 0.2]);

    const run = createEmbedDelta({
      dbPath: '/db',
      DatabaseCtor: ctor as unknown as new (p: string, o?: unknown) => unknown as never,
      getLanceStore: () => ({ db: null, table: lanceTable as never }),
      setLanceTable: jest.fn(),
      embed,
      minLength: MIN_EMBED_LENGTH,
      pageSize: 10,
      log: jest.fn(),
      error: jest.fn(),
    });

    const r = await run();

    expect(r.embedded).toBe(1);
    expect(embed).toHaveBeenCalledTimes(1);
    expect(state.markedIds.has(42)).toBe(true);
  });

  test('production MIN_EMBED_LENGTH admits a one-word "fuck" reply through createEmbedDelta', async () => {
    const jeffMessage: FakeMsg = {
      id: 99, source: 'claude', channel: 'session:silas', role: 'jeff',
      content: 'fuck', timestamp: '2026-05-06T18:31:00Z',
    };
    const { ctor, state } = filteringDbFactory([jeffMessage]);
    const lanceTable = { add: jest.fn() };
    const embed = jest.fn(async (_t: string) => [0.3]);

    const run = createEmbedDelta({
      dbPath: '/db',
      DatabaseCtor: ctor as unknown as new (p: string, o?: unknown) => unknown as never,
      getLanceStore: () => ({ db: null, table: lanceTable as never }),
      setLanceTable: jest.fn(),
      embed,
      minLength: MIN_EMBED_LENGTH,
      pageSize: 10,
      log: jest.fn(),
      error: jest.fn(),
    });

    const r = await run();
    expect(r.embedded).toBe(1);
    expect(state.markedIds.has(99)).toBe(true);
  });
});
