/**
 * chorus-reprompt-analytics handler — unit tests (#2188).
 */
import Database from 'better-sqlite3';
import { fetchChorusRepromptAnalytics, type ChorusRepromptAnalyticsDeps } from '../../src/handlers/chorus-reprompt-analytics';

const FIXED_NOW = new Date('2026-04-18T12:00:00Z').getTime();
const nowFn = () => FIXED_NOW;

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

function add(db: Database.Database, m: { content: string; channel: string; timestamp: string; author?: string; source?: string }) {
  db.prepare('INSERT INTO messages (author, source, channel, content, timestamp) VALUES (?,?,?,?,?)')
    .run(m.author || 'user', m.source || 'claude', m.channel, m.content, m.timestamp);
}

function deps(db: Database.Database): ChorusRepromptAnalyticsDeps {
  return { db, now: nowFn };
}

describe('fetchChorusRepromptAnalytics (#2188)', () => {
  test('empty DB → 200 zero headline', () => {
    const db = seedDb();
    const r = fetchChorusRepromptAnalytics(deps(db), {});
    expect(r.status).toBe(200);
    const body = r.body as { headline: { totalMessages: number; totalSignals: number; signalRate: number } };
    expect(body.headline.totalMessages).toBe(0);
    expect(body.headline.totalSignals).toBe(0);
    expect(body.headline.signalRate).toBe(0);
    db.close();
  });

  test('REPROMPT_KEYWORDS classified as reprompt', () => {
    const db = seedDb();
    add(db, { content: 'i told you already fix it', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as {
      headline: { reprompt: number }; byRole: Record<string, { reprompt: number }>;
    };
    expect(body.headline.reprompt).toBe(1);
    expect(body.byRole.wren.reprompt).toBe(1);
    db.close();
  });

  test('"run it again" false positive is dropped', () => {
    const db = seedDb();
    add(db, { content: 'run it again to confirm', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as { headline: { reprompt: number } };
    expect(body.headline.reprompt).toBe(0);
    db.close();
  });

  test('APPROVAL bigrams classified as approval', () => {
    const db = seedDb();
    add(db, { content: 'yes please go ahead', channel: 'session:silas', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as {
      headline: { approvalOverhead: number }; byRole: Record<string, { approval: number }>;
    };
    expect(body.headline.approvalOverhead).toBe(1);
    expect(body.byRole.silas.approval).toBe(1);
    db.close();
  });

  test('CORRECTION patterns classified as correction when <100 chars', () => {
    const db = seedDb();
    add(db, { content: "don't do that", channel: 'session:kade', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as { headline: { correction: number } };
    expect(body.headline.correction).toBe(1);
    db.close();
  });

  test('CORRECTION patterns over 100 chars skipped', () => {
    const db = seedDb();
    add(db, { content: "don't do that " + 'x'.repeat(100), channel: 'session:kade', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as { headline: { correction: number } };
    expect(body.headline.correction).toBe(0);
    db.close();
  });

  test('system-reminder block stripped before classification', () => {
    const db = seedDb();
    add(db, {
      content: '<system-reminder>long ignored</system-reminder>\nyes please go ahead',
      channel: 'session:wren',
      timestamp: '2026-04-18T10:00:00Z',
    });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as { headline: { approvalOverhead: number } };
    expect(body.headline.approvalOverhead).toBe(1);
    db.close();
  });

  test('non-claude or non-session rows excluded', () => {
    const db = seedDb();
    add(db, { content: 'yes please go', channel: 'session:wren', source: 'slack', timestamp: '2026-04-18T10:00:00Z' });
    add(db, { content: 'yes please go', channel: '#team', source: 'claude', timestamp: '2026-04-18T10:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as { headline: { totalMessages: number } };
    expect(body.headline.totalMessages).toBe(0);
    db.close();
  });

  test('trend sorted by day ASC with attentionCost weighting', () => {
    const db = seedDb();
    add(db, { content: 'again and again', channel: 'session:wren', timestamp: '2026-04-18T10:00:00Z' }); // reprompt (3)
    add(db, { content: 'wrong', channel: 'session:wren', timestamp: '2026-04-18T11:00:00Z' }); // correction (2)
    add(db, { content: 'yes please', channel: 'session:wren', timestamp: '2026-04-17T10:00:00Z' }); // approval (1)
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as {
      trend: Array<{ date: string; attentionCost: number }>;
    };
    expect(body.trend.map((t) => t.date)).toEqual(['2026-04-17', '2026-04-18']);
    expect(body.trend[0].attentionCost).toBe(1);
    expect(body.trend[1].attentionCost).toBe(5); // 3 + 2
    db.close();
  });

  test('days param clamps to [1, 365] and flows to meta', () => {
    const db = seedDb();
    const high = fetchChorusRepromptAnalytics(deps(db), { days: '9999' }).body as { meta: { days: number } };
    expect(high.meta.days).toBe(365);
    const low = fetchChorusRepromptAnalytics(deps(db), { days: '0' }).body as { meta: { days: number } };
    expect(low.meta.days).toBe(1);
    const mid = fetchChorusRepromptAnalytics(deps(db), { days: '42' }).body as { meta: { days: number } };
    expect(mid.meta.days).toBe(42);
    db.close();
  });

  test('recentEvents limited to 20, most recent first', () => {
    const db = seedDb();
    for (let i = 0; i < 25; i++) {
      add(db, {
        content: 'again and again',
        channel: 'session:wren',
        timestamp: `2026-04-18T${String(i).padStart(2, '0')}:00:00Z`,
      });
    }
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as {
      recentEvents: Array<{ timestamp: string }>;
    };
    expect(body.recentEvents.length).toBe(20);
    // Most recent first
    expect(body.recentEvents[0].timestamp > body.recentEvents[19].timestamp).toBe(true);
    db.close();
  });

  test('topPhrases ordered DESC by count and capped at 15', () => {
    const db = seedDb();
    for (let i = 0; i < 3; i++) {
      add(db, { content: 'again common phrase', channel: 'session:wren', timestamp: `2026-04-18T${i}0:00:00Z` });
    }
    add(db, { content: 'still saying something else', channel: 'session:wren', timestamp: '2026-04-18T05:00:00Z' });
    const body = fetchChorusRepromptAnalytics(deps(db), {}).body as {
      topPhrases: Array<{ phrase: string; count: number }>;
    };
    expect(body.topPhrases[0].count).toBe(3);
    expect(body.topPhrases.length).toBeLessThanOrEqual(15);
    db.close();
  });
});
