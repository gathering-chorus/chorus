/**
 * chorus-voice-analytics handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusVoiceAnalytics, classifyTone, type ChorusVoiceAnalyticsDeps } from '../../src/handlers/chorus-voice-analytics';

const FIXED_NOW = new Date('2026-04-18T12:00:00Z').getTime();

function seedDb(): Database.Database {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE messages (
      id INTEGER PRIMARY KEY,
      author TEXT, source TEXT, channel TEXT, content TEXT, timestamp TEXT
    );
  `);
  return db;
}

function add(db: Database.Database, m: { content: string; channel: string; timestamp: string }) {
  db.prepare('INSERT INTO messages (author, source, channel, content, timestamp) VALUES (?,?,?,?,?)')
    .run('user', 'claude', m.channel, m.content, m.timestamp);
}

function deps(db: Database.Database, over: Partial<ChorusVoiceAnalyticsDeps> = {}): ChorusVoiceAnalyticsDeps {
  return { db, isEDT: () => true, now: () => FIXED_NOW, ...over };
}

describe('classifyTone', () => {
  test('corrective first', () => {
    expect(classifyTone("don't do that")).toBe('corrective');
    expect(classifyTone('that is wrong')).toBe('corrective');
  });
  test('question when ? present and not corrective', () => {
    expect(classifyTone('is this ok?')).toBe('question');
  });
  test('directive when first word is imperative verb', () => {
    expect(classifyTone('fix the bug')).toBe('directive');
    expect(classifyTone('deploy now')).toBe('directive');
  });
  test('collaborative', () => {
    expect(classifyTone("let's try")).toBe('collaborative');
  });
  test('acknowledgment', () => {
    expect(classifyTone('ok got it')).toBe('acknowledgment');
    expect(classifyTone('perfect')).toBe('acknowledgment');
  });
  test('routing', () => {
    expect(classifyTone('take a look at localhost')).toBe('routing');
  });
  test('status', () => {
    expect(classifyTone('I just shipped it')).toBe('status');
  });
  test('narrative is fallback', () => {
    expect(classifyTone('mushroom hunting weather')).toBe('narrative');
  });
});

describe('fetchChorusVoiceAnalytics (#2188)', () => {
  test('empty DB → zero headline, null date range', () => {
    const db = seedDb();
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as {
      meta: { messages: number; dateRange: { from: null; to: null } };
      headline: Record<string, number>;
    };
    expect(body.meta.messages).toBe(0);
    expect(body.meta.dateRange.from).toBeNull();
    expect(body.headline.narrative).toBe(0);
    db.close();
  });

  test('filters system-reminder, long, JSON/XML, path, single-word, skill injections', () => {
    const db = seedDb();
    add(db, { content: '<system-reminder>x</system-reminder>mushroom season', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    add(db, { content: 'x'.repeat(600), channel: 'session:wren', timestamp: '2026-04-18T10:01:00Z' });
    add(db, { content: '<xml>', channel: 'session:wren', timestamp: '2026-04-18T10:02:00Z' });
    add(db, { content: '{json}', channel: 'session:wren', timestamp: '2026-04-18T10:03:00Z' });
    add(db, { content: '/Users/jeff/file', channel: 'session:wren', timestamp: '2026-04-18T10:04:00Z' });
    add(db, { content: 'yes', channel: 'session:wren', timestamp: '2026-04-18T10:05:00Z' });
    add(db, { content: 'Base directory for this skill: /foo', channel: 'session:wren', timestamp: '2026-04-18T10:06:00Z' });
    add(db, { content: 'fix the pipeline', channel: 'session:wren', timestamp: '2026-04-18T10:07:00Z' });
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as { meta: { messages: number } };
    // Only "mushroom season" (after strip) and "fix the pipeline" survive
    expect(body.meta.messages).toBe(2);
    db.close();
  });

  test('days clamps to [1,365]', () => {
    const db = seedDb();
    const hi = fetchChorusVoiceAnalytics(deps(db), { days: '9999' }).body as { meta: { days: number } };
    expect(hi.meta.days).toBe(365);
    const lo = fetchChorusVoiceAnalytics(deps(db), { days: '0' }).body as { meta: { days: number } };
    expect(lo.meta.days).toBe(1);
    db.close();
  });

  test('headline percentages sum to approximately 100 for filtered set', () => {
    const db = seedDb();
    add(db, { content: 'fix the build', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' }); // directive
    add(db, { content: 'is it ready?', channel: 'session:wren', timestamp: '2026-04-18T10:01:00Z' }); // question
    add(db, { content: "don't merge yet", channel: 'session:wren', timestamp: '2026-04-18T10:02:00Z' }); // corrective
    add(db, { content: 'perfect work', channel: 'session:wren', timestamp: '2026-04-18T10:03:00Z' }); // acknowledgment
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as { headline: Record<string, number> };
    const sum = Object.values(body.headline).reduce((a, b) => a + b, 0);
    expect(sum).toBeGreaterThanOrEqual(99);
    expect(sum).toBeLessThanOrEqual(101);
    expect(body.headline.directive).toBe(25);
    expect(body.headline.question).toBe(25);
  });

  test('toneByRole percentages only count within-role denominators', () => {
    const db = seedDb();
    add(db, { content: 'fix it', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    add(db, { content: 'run it', channel: 'session:silas', timestamp: '2026-04-18T10:01:00Z' });
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as {
      toneByRole: Record<string, Record<string, number>>;
    };
    expect(body.toneByRole.wren.directive).toBe(100);
    expect(body.toneByRole.silas.directive).toBe(100);
    expect(body.toneByRole.kade.directive).toBe(0);
  });

  test('hour-of-day uses injected isEDT offset (UTC-4)', () => {
    const db = seedDb();
    add(db, { content: 'fix the build', channel: 'session:wren', timestamp: '2026-04-18T14:00:00Z' });
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as {
      hourOfDay: Record<string, number[]>;
    };
    // UTC 14:00 with EDT offset 4 → 10 Boston
    expect(body.hourOfDay.wren[10]).toBe(1);
  });

  test('bigrams capped at 25 and sorted DESC', () => {
    const db = seedDb();
    for (let i = 0; i < 30; i++) {
      add(db, {
        content: `build pipeline step ${i} phase`,
        channel: 'session:wren',
        timestamp: `2026-04-18T${String(i % 24).padStart(2, '0')}:00:00Z`,
      });
    }
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as { bigrams: Array<{ count: number }> };
    expect(body.bigrams.length).toBeLessThanOrEqual(25);
    for (let i = 0; i < body.bigrams.length - 1; i++) {
      expect(body.bigrams[i].count).toBeGreaterThanOrEqual(body.bigrams[i + 1].count);
    }
  });

  test('dateRange uses first/last filtered timestamps', () => {
    const db = seedDb();
    add(db, { content: 'fix the build', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    add(db, { content: 'deploy now', channel: 'session:wren', timestamp: '2026-04-19T11:00:00Z' });
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as {
      meta: { dateRange: { from: string; to: string } };
    };
    expect(body.meta.dateRange.from).toBe('2026-04-18');
    expect(body.meta.dateRange.to).toBe('2026-04-19');
  });

  test('corrective-only rows contribute to correctiveWords list', () => {
    const db = seedDb();
    add(db, { content: "don't deploy broken builds today", channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    add(db, { content: 'fix the broken builds today', channel: 'session:wren', timestamp: '2026-04-18T11:00:00Z' });
    const body = fetchChorusVoiceAnalytics(deps(db), {}).body as {
      correctiveWords: Array<{ word: string }>;
    };
    const words = new Set(body.correctiveWords.map((w) => w.word));
    expect(words.has('broken')).toBe(true);
    expect(words.has('builds')).toBe(true);
  });
});
