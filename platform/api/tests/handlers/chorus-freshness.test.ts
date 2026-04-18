/**
 * chorus-freshness handler — unit tests (#2189).
 *
 * In-memory SQLite seeded with watermarks + messages, fake fs for spine log,
 * injected `now` for deterministic age math.
 *
 * Tests verify:
 *   - drift-path (claude/spine): 0 drift → fresh, <100 → warn, <1000 → critical, 1000+ → dead
 *   - ratio-path (other sources): ratio ≤1.5 fresh, ≤3 warn, ≤7 critical, else dead
 *   - aggregation: 'claude:wren' + 'claude:silas' roll up under 'claude' with latest timestamp
 *   - 'artifact:adr:xyz' keeps 'artifact:adr' prefix (two-level for artifact source)
 *   - summary counts align with sources[].level
 *   - missing spine log → onDisk=0, drift path works
 *   - unknown source → default cadence 86400
 */
import Database from 'better-sqlite3';
import { fetchFreshness, type FreshnessDeps } from '../../src/handlers/chorus-freshness';

function emptyDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (id INTEGER PRIMARY KEY, source TEXT);
    CREATE TABLE watermarks (source TEXT PRIMARY KEY, last_indexed TEXT);
  `);
  return db;
}

const FIXED_NOW_MS = new Date('2026-04-18T14:00:00Z').getTime();

function deps(db: Database.Database, overrides: Partial<FreshnessDeps> = {}): FreshnessDeps {
  return {
    db,
    exists: () => false,
    readFile: () => '',
    now: () => FIXED_NOW_MS,
    timestamp: () => '2026-04-18T10:00:00-04:00',
    spineLogPath: '/tmp/fake-spine.log',
    ...overrides,
  };
}

describe('fetchFreshness (#2189 /api/chorus/freshness)', () => {
  test('claude drift 0 → fresh; both message count and watermark count align', () => {
    const db = emptyDb();
    db.prepare('INSERT INTO watermarks VALUES (?, ?)').run(
      'claude:wren',
      '2026-04-18T13:59:00Z',
    );
    db.prepare('INSERT INTO messages (source) VALUES (?)').run('claude');
    const r = fetchFreshness(deps(db));
    expect(r.status).toBe(200);
    const b = r.body as { sources: Array<Record<string, unknown>>; summary: Record<string, number> };
    const claude = b.sources.find((s) => s.source === 'claude');
    expect(claude?.level).toBe('fresh');
    expect(claude?.unindexed).toBe(0);
    expect(b.summary.fresh).toBe(1);
  });

  test('claude drift 50 → warn (on-disk 50 watermarks, 0 indexed messages)', () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO watermarks VALUES (?, ?)');
    for (let i = 0; i < 50; i++) ins.run(`claude:s${i}`, '2026-04-18T13:00:00Z');
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; level: string; unindexed: number }> };
    const claude = b.sources.find((s) => s.source === 'claude');
    expect(claude?.level).toBe('warn');
    expect(claude?.unindexed).toBe(50);
  });

  test('claude drift 500 → critical', () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO watermarks VALUES (?, ?)');
    for (let i = 0; i < 500; i++) ins.run(`claude:s${i}`, '2026-04-18T13:00:00Z');
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; level: string }> };
    expect(b.sources.find((s) => s.source === 'claude')?.level).toBe('critical');
  });

  test('claude drift 2000 → dead', () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO watermarks VALUES (?, ?)');
    for (let i = 0; i < 2000; i++) ins.run(`claude:s${i}`, '2026-04-18T13:00:00Z');
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; level: string; unindexed: number }> };
    const claude = b.sources.find((s) => s.source === 'claude');
    expect(claude?.level).toBe('dead');
    expect(claude?.unindexed).toBe(2000);
  });

  test('brief ratio path: 1h old vs 24h cadence → fresh', () => {
    const db = emptyDb();
    // 1h old at fixed now: 2026-04-18T13:00:00Z
    db.prepare('INSERT INTO watermarks VALUES (?, ?)').run('brief', '2026-04-18T13:00:00Z');
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; level: string; expected_cadence: number }> };
    const brief = b.sources.find((s) => s.source === 'brief');
    expect(brief?.level).toBe('fresh');
    expect(brief?.expected_cadence).toBe(86400);
  });

  test('brief ratio path: 10 days old vs 24h cadence (ratio=10) → dead', () => {
    const db = emptyDb();
    const tenDaysAgo = new Date(FIXED_NOW_MS - 10 * 86400_000).toISOString();
    db.prepare('INSERT INTO watermarks VALUES (?, ?)').run('brief', tenDaysAgo);
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; level: string }> };
    expect(b.sources.find((s) => s.source === 'brief')?.level).toBe('dead');
  });

  test("aggregates 'claude:wren' + 'claude:silas' under 'claude' with latest timestamp", () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO watermarks VALUES (?, ?)');
    ins.run('claude:wren', '2026-04-18T10:00:00Z');
    ins.run('claude:silas', '2026-04-18T13:30:00Z');
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; last_indexed: string }> };
    const claudes = b.sources.filter((s) => s.source === 'claude');
    expect(claudes).toHaveLength(1);
    expect(claudes[0].last_indexed).toBe('2026-04-18T13:30:00Z');
  });

  test("'artifact:adr:xyz' keeps 'artifact:adr' prefix", () => {
    const db = emptyDb();
    db.prepare('INSERT INTO watermarks VALUES (?, ?)').run(
      'artifact:adr:ADR-001',
      '2026-04-18T13:00:00Z',
    );
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string }> };
    expect(b.sources.map((s) => s.source)).toContain('artifact:adr');
  });

  test('unknown source → default cadence 86400', () => {
    const db = emptyDb();
    db.prepare('INSERT INTO watermarks VALUES (?, ?)').run(
      'unknownthing',
      '2026-04-18T13:00:00Z',
    );
    const r = fetchFreshness(deps(db));
    const b = r.body as { sources: Array<{ source: string; expected_cadence: number }> };
    expect(b.sources.find((s) => s.source === 'unknownthing')?.expected_cadence).toBe(86400);
  });

  test('summary counts align with source levels', () => {
    const db = emptyDb();
    const ins = db.prepare('INSERT INTO watermarks VALUES (?, ?)');
    ins.run('brief', '2026-04-18T13:30:00Z'); // fresh
    ins.run('decision', new Date(FIXED_NOW_MS - 2 * 86400_000).toISOString()); // 2d vs 24h = ratio 2 → warn
    ins.run('memory', new Date(FIXED_NOW_MS - 10 * 86400_000).toISOString()); // ratio 10 → dead
    const r = fetchFreshness(deps(db));
    const b = r.body as { summary: Record<string, number> };
    expect(b.summary.fresh).toBe(1);
    expect(b.summary.warn).toBe(1);
    expect(b.summary.dead).toBe(1);
    expect(b.summary.total_sources).toBe(3);
  });
});
