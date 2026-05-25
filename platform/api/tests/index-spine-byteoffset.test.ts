// #3077 AC2: indexSpine must read only NEW bytes since the last cycle (byte-offset
// watermark), not re-parse the whole 16MB tail every reindex. Proves: first cycle
// indexes all + persists an offset; second cycle (after append) indexes ONLY the
// new events and calls readSince with the stored offset; rotation (size<offset) resets.
import { createIndexAllSources } from '../src/index-all-sources';

// Stateful fake DB: a watermark store shared across run() invocations (the real
// reindex persists the offset between ~15-min cycles via the watermarks table).
function statefulDbFactory() {
  const runs: Array<{ sql: string; args: unknown[] }> = [];
  const watermarks = new Map<string, string>(); // source -> last_indexed
  const ctor = jest.fn((_p: string) => ({
    pragma: jest.fn(),
    prepare: (sql: string) => ({
      run: (...args: unknown[]) => {
        runs.push({ sql, args });
        // mirror the updateWatermark INSERT ... ON CONFLICT (source, last_seen, last_indexed)
        if (sql.includes('INTO watermarks') && typeof args[0] === 'string') {
          watermarks.set(args[0] as string, String(args[2]));
        }
        return undefined;
      },
      get: (...args: unknown[]) => {
        if (sql.includes('FROM watermarks') && sql.includes('last_indexed')) {
          const src = args[0] as string;
          const v = watermarks.get(src);
          return v === undefined ? undefined : { last_indexed: v };
        }
        return undefined;
      },
    }),
    transaction: (fn: (events: unknown[]) => void) => (events: unknown[]) => fn(events),
    close: jest.fn(),
  }));
  return { ctor, runs, watermarks };
}

const FAKE_PATH = { join: (...parts: string[]) => parts.join('/') };
const baseFs = () => ({
  existsSync: (p: string) => p.endsWith('chorus.log'),
  readdirSync: jest.fn(() => [] as string[]),
  readFileSync: jest.fn(() => ''),
  statSync: jest.fn(() => ({ mtime: { toISOString: () => 't' } })),
});

const evt = (ts: string, event: string, role = 'kade') =>
  JSON.stringify({ timestamp: ts, event, role });

describe('#3077 AC2 — indexSpine byte-offset watermark (incremental)', () => {
  it('first cycle indexes all events and persists the byte offset', async () => {
    const { ctor, runs, watermarks } = statefulDbFactory();
    const log = [evt('2026-05-25T00:00:00Z', 'card.pulled'), evt('2026-05-25T00:01:00Z', 'card.accepted')].join('\n') + '\n';
    const readSince = jest.fn((_p: string, offset: number) => ({
      content: offset === 0 ? log : '',
      startOffset: offset > Buffer.byteLength(log) ? 0 : offset,
      size: Buffer.byteLength(log),
    }));
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: baseFs() as any,
      path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h', readSince: readSince as any,
    });
    const r = await run();
    expect(r.indexed.spine).toMatch(/\d+ events indexed/);
    const inserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'spine');
    expect(inserts.length).toBe(2);
    expect(readSince).toHaveBeenCalledWith(expect.stringContaining('chorus.log'), 0);
    expect(watermarks.get('spine:offset')).toBe(String(Buffer.byteLength(log)));
  });

  it('second cycle reads from the stored offset and indexes ONLY new events', async () => {
    const { ctor, runs, watermarks } = statefulDbFactory();
    const log1 = [evt('2026-05-25T00:00:00Z', 'card.pulled'), evt('2026-05-25T00:01:00Z', 'card.accepted')].join('\n') + '\n';
    const newLine = evt('2026-05-25T00:02:00Z', 'card.demo.started') + '\n';
    const off1 = Buffer.byteLength(log1);
    const readSince = jest.fn((_p: string, offset: number) => {
      if (offset === 0) return { content: log1, startOffset: 0, size: off1 };
      // appended one new line; only the new bytes are returned
      return { content: newLine, startOffset: offset, size: off1 + Buffer.byteLength(newLine) };
    });
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: baseFs() as any,
      path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h', readSince: readSince as any,
    });
    await run(); // cycle 1 — indexes 2, persists offset
    runs.length = 0; // clear, measure only cycle 2
    await run(); // cycle 2 — should index only the 1 new event
    const inserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'spine');
    expect(inserts.length).toBe(1); // NOT 3 — the old two are not re-parsed/re-inserted
    expect(readSince).toHaveBeenLastCalledWith(expect.stringContaining('chorus.log'), off1);
    expect(watermarks.get('spine:offset')).toBe(String(off1 + Buffer.byteLength(newLine)));
  });

  it('resets the offset to 0 when the log was rotated (size < stored offset)', async () => {
    const { ctor, runs, watermarks } = statefulDbFactory();
    watermarks.set('spine:offset', '999999'); // stale large offset from before rotation
    const smallLog = evt('2026-05-25T01:00:00Z', 'card.pulled') + '\n';
    const readSince = jest.fn((_p: string, offset: number) => ({
      // rotation: stored offset exceeds the new (small) file size → reader resets to 0
      content: smallLog,
      startOffset: offset > Buffer.byteLength(smallLog) ? 0 : offset,
      size: Buffer.byteLength(smallLog),
    }));
    const run = createIndexAllSources({
      dbPath: '/db', DatabaseCtor: ctor as any, fs: baseFs() as any,
      path: FAKE_PATH as any, repoRoot: '/r', homedir: () => '/h', readSince: readSince as any,
    });
    await run();
    const inserts = runs.filter(e => e.sql.includes('INTO messages') && e.args[0] === 'spine');
    expect(inserts.length).toBe(1); // re-indexed from the rotated file's start, no crash
    expect(watermarks.get('spine:offset')).toBe(String(Buffer.byteLength(smallLog)));
  });
});
