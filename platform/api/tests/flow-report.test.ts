// @test-type: unit — pure parser/formatter fixtures; no Loki, no live services.
/**
 * flow-report core — unit tests (#3269).
 *
 * The card cycle/step/error fitness function as a pure aggregation: spine
 * events in → structured JSON out (per-card cycle, step times, errors as
 * children, ranked error classes). The instrument behind #3266's walk-away
 * bar, replacing the 06-06 one-off bash→Loki→HTML report.
 */
import { aggregateFlow, FlowEvent, deriveWalkAway } from '../src/flow-report';
import { normalizeLine, esc, buildHtml } from '../src/flow-report-cli';

const T0 = Date.parse('2026-06-10T10:00:00-04:00');
const MIN = 60_000;

function ev(card: number, event: string, minsAfter: number, extra: Partial<FlowEvent> = {}): FlowEvent {
  return { ts: T0 + minsAfter * MIN, event, card_id: card, role: 'silas', detail: '', ...extra };
}

describe('aggregateFlow (#3269)', () => {
  test('AC1: per-card cycle + step times from lifecycle events', () => {
    const events: FlowEvent[] = [
      ev(101, 'pull.completed', 0),
      ev(101, 'commit.completed', 28),   // work: 28m
      ev(101, 'push.completed', 29),     // push: 1m
      ev(101, 'build.completed', 32),    // build: 3m
      ev(101, 'deploy.completed', 33),   // deploy: 1m
      ev(101, 'werk.presented', 34),     // demo: 1m
      ev(101, 'werk.landed', 50),        // merge: 16m (includes the human wait)
      ev(101, 'card.accepted', 51),      // final: 1m
    ];
    const r = aggregateFlow(events);
    expect(r.cards).toHaveLength(1);
    const c = r.cards[0];
    expect(c.card).toBe(101);
    expect(c.landed).toBe(true);
    expect(c.cycleS).toBe(51 * 60);
    expect(c.steps.workS).toBe(28 * 60);
    expect(c.steps.pushS).toBe(1 * 60);
    expect(c.steps.buildS).toBe(3 * 60);
    expect(c.steps.deployS).toBe(1 * 60);
    expect(c.steps.demoS).toBe(1 * 60);
    expect(c.steps.mergeS).toBe(16 * 60);
    expect(c.steps.finalS).toBe(1 * 60);
  });

  test('AC1: errors/warnings enumerate as children of their card', () => {
    const events: FlowEvent[] = [
      ev(102, 'pull.completed', 0),
      ev(102, 'demo.refused', 5, { detail: 'gates-missing' }),
      ev(102, 'build.failed', 7, { detail: 'rustc exploded' }),
      ev(102, 'commit.completed', 10),
    ];
    const r = aggregateFlow(events);
    const c = r.cards[0];
    expect(c.landed).toBe(false);
    expect(c.errors).toHaveLength(2);
    expect(c.errors[0]).toMatchObject({ event: 'demo.refused', detail: 'gates-missing' });
    expect(c.errors[1].event).toBe('build.failed');
  });

  test('error classes ranked across cards (the demo.refused finding)', () => {
    const events: FlowEvent[] = [
      ev(103, 'demo.refused', 1), ev(103, 'demo.refused', 2),
      ev(104, 'demo.refused', 3),
      ev(104, 'deploy.failed', 4),
      ev(103, 'pull.completed', 0), ev(104, 'pull.completed', 0),
    ];
    const r = aggregateFlow(events);
    expect(r.errorClasses[0]).toMatchObject({ event: 'demo.refused', count: 3 });
    expect(r.errorClasses[1]).toMatchObject({ event: 'deploy.failed', count: 1 });
  });

  test('missing steps render null, not zero or NaN; partial cards still report', () => {
    const events: FlowEvent[] = [
      ev(105, 'pull.completed', 0),
      ev(105, 'commit.completed', 10),
      // never pushed/built — card stalled
    ];
    const r = aggregateFlow(events);
    const c = r.cards[0];
    expect(c.steps.workS).toBe(600);
    expect(c.steps.pushS).toBeNull();
    expect(c.steps.buildS).toBeNull();
    expect(c.landed).toBe(false);
    expect(c.cycleS).toBe(600); // first→last observed
  });

  test('repeated step events take the LAST occurrence (retries supersede)', () => {
    const events: FlowEvent[] = [
      ev(106, 'pull.completed', 0),
      ev(106, 'commit.completed', 5),
      ev(106, 'push.completed', 6),
      ev(106, 'build.completed', 8),
      ev(106, 'build.completed', 20), // rebuild after a fix
      ev(106, 'deploy.completed', 21),
    ];
    const r = aggregateFlow(events);
    const c = r.cards[0];
    expect(c.steps.buildS).toBe(14 * 60); // push@6 → last build@20
    expect(c.steps.deployS).toBe(1 * 60);
  });

  test('cards sorted newest-first by last event; totals summarize', () => {
    const events: FlowEvent[] = [
      ev(107, 'pull.completed', 0), ev(107, 'card.accepted', 10),
      ev(108, 'pull.completed', 20), ev(108, 'demo.refused', 25),
    ];
    const r = aggregateFlow(events);
    expect(r.cards.map((c) => c.card)).toEqual([108, 107]);
    expect(r.totals).toMatchObject({ cards: 2, landed: 1, withErrors: 1, errorEvents: 1 });
  });

  test('out-of-order checkpoints render null, never negative (duplicate-land class)', () => {
    const events: FlowEvent[] = [
      ev(110, 'pull.completed', 0),
      ev(110, 'werk.landed', 10),       // land fired first (phantom-go attempt)
      ev(110, 'deploy.completed', 15),  // a later deploy event crosses it
      ev(110, 'card.accepted', 16),
    ];
    const r = aggregateFlow(events);
    const c = r.cards[0];
    expect(c.steps.mergeS).toBeNull();  // landed@10 < deploy@15 → boundary untrustworthy
    expect(c.steps.finalS).toBe(6 * 60); // accepted@16 - landed@10, still ordered
    expect(c.landed).toBe(true);
  });

  test('#3397: implausibly-clocked events never poison cycleS/mergeS (timing-only sanitize)', () => {
    const ANCIENT = 17811076913; // ~1970 — the BSD %3N corruption magnitude (#3266)
    const FUTURE = Date.parse('2081-01-01T00:00:00Z'); // ~2081 — cross-source clock skew
    const events: FlowEvent[] = [
      ev(301, 'pull.completed', 0),
      ev(301, 'commit.completed', 10), // work: 10m
      ev(301, 'deploy.completed', 12), // deploy: 2m
      // poison-clocked error events — must be ignored for TIMING, kept for errors:
      { ts: ANCIENT, event: 'werk.land.failed', card_id: 301, role: 'silas', detail: 'bsd-3N' },
      ev(301, 'werk.landed', 20), // merge: 8m, a real 2026 land
      ev(301, 'card.accepted', 21), // final: 1m
      { ts: FUTURE, event: 'deploy.cdhash.diverged', card_id: 301, role: 'silas', detail: 'skew' },
    ];
    const r = aggregateFlow(events);
    const c = r.cards[0];
    // cycle + steps bounded by plausible clocks (pull@0 → accepted@21), not 1970/2081
    expect(c.cycleS).toBe(21 * 60); // NOT a 55-year value
    expect(c.steps.workS).toBe(10 * 60);
    expect(c.steps.deployS).toBe(2 * 60);
    expect(c.steps.mergeS).toBe(8 * 60); // from werk.landed@20, not poisoned to billions
    expect(c.lastEventTs).toBe(T0 + 21 * MIN); // the future event did not become "last"
    // timing-only: the poison events still count as errors and still flip landed
    expect(c.landed).toBe(true);
    expect(c.errors).toHaveLength(2);
    expect(c.errors.map((e) => e.event)).toEqual(
      expect.arrayContaining(['werk.land.failed', 'deploy.cdhash.diverged']),
    );
  });

  test('events without card_id are ignored (ambient noise is not a card)', () => {
    const events: FlowEvent[] = [
      ev(109, 'pull.completed', 0),
      { ts: T0, event: 'system.heartbeat', card_id: undefined, role: 'silas', detail: '' },
    ];
    const r = aggregateFlow(events);
    expect(r.cards).toHaveLength(1);
  });
});

describe('cycleStats (#3269) — overall cycle as leading indicator', () => {
  test('median/avg/p90 over LANDED cards only; empty-safe', () => {
    const events: FlowEvent[] = [
      ev(201, 'pull.completed', 0), ev(201, 'card.accepted', 10),   // 600s
      ev(202, 'pull.completed', 0), ev(202, 'card.accepted', 20),   // 1200s
      ev(203, 'pull.completed', 0), ev(203, 'card.accepted', 100),  // 6000s
      ev(204, 'pull.completed', 0), ev(204, 'demo.refused', 5),     // not landed
    ];
    const r = aggregateFlow(events);
    expect(r.cycleStats.landedCards).toBe(3);
    expect(r.cycleStats.medianS).toBe(1200);
    expect(r.cycleStats.avgS).toBe(2600);
    expect(r.cycleStats.p90S).toBe(6000);
    expect(aggregateFlow([]).cycleStats).toEqual({ landedCards: 0, medianS: null, avgS: null, p90S: null });
  });

  test('#3397: a poison-clocked landed card does not blow cycleStats (the 55-year-median bug)', () => {
    const events: FlowEvent[] = [
      ev(401, 'pull.completed', 0), ev(401, 'card.accepted', 10), // 600s
      ev(402, 'pull.completed', 0), ev(402, 'card.accepted', 20), // 1200s
      // a LANDED card carrying a ~1970 poison event that, unguarded, would make
      // its cycleS ~55 years and drag the median to the "55-year" page bug:
      ev(403, 'pull.completed', 0),
      { ts: 17811076913, event: 'werk.land.failed', card_id: 403, role: 'silas', detail: 'bsd-3N' },
      ev(403, 'werk.landed', 30), ev(403, 'card.accepted', 30), // real cycle 1800s
    ];
    const r = aggregateFlow(events);
    const c403 = r.cards.find((c) => c.card === 403);
    expect(c403?.cycleS).toBe(30 * 60); // bounded to its real span, not 55 years
    expect(c403?.landed).toBe(true);
    // median over the real cycles [600,1200,1800] = 1200 — NOT a billion-second value
    expect(r.cycleStats.medianS).toBe(1200);
    expect(r.cycleStats.p90S).toBeLessThan(10_000); // sane, never a 55-year p90
  });
});

describe('normalizeLine (#3269) — both Loki source shapes', () => {
  test('werk-verbs jsonl shape (ts epoch-ms)', () => {
    const e = normalizeLine('{"ts":1781131618313,"event":"build.completed","role":"kade","card_id":3299,"name":"werk-pull"}');
    expect(e).toMatchObject({ ts: 1781131618313, event: 'build.completed', card_id: 3299, role: 'kade', detail: 'werk-pull' });
  });
  test('platform-chorus spine shape (ISO timestamp)', () => {
    const e = normalizeLine('{"timestamp":"2026-06-10T18:14:33.320-0400","event":"card.accepted","role":"silas","card_id":3334}');
    expect(e?.event).toBe('card.accepted');
    expect(e?.card_id).toBe(3334);
    expect(e?.ts).toBe(Date.parse('2026-06-10T18:14:33.320-0400'));
  });
  test('garbage and event-less lines are null, never throw', () => {
    expect(normalizeLine('not json')).toBeNull();
    expect(normalizeLine('{"timestamp":"2026-06-10T00:00:00Z","level":"info"}')).toBeNull();
    expect(normalizeLine('{"event":"x"}')).toBeNull(); // no timestamp at all
  });
  test('detail prefers reason/error fields, truncates at 200', () => {
    const e = normalizeLine(`{"ts":1,"event":"deploy.failed","card_id":7,"reason":"${'x'.repeat(300)}"}`);
    expect(e?.detail?.length).toBe(200);
  });
});

describe('buildHtml escaping (#3269 gate-quality catch, held through the interactive rewrite)', () => {
  test('log-sourced detail/event cannot inject markup or break out of the embedded JSON', () => {
    expect(esc('<img src=x onerror=alert(1)>')).toBe('&lt;img src=x onerror=alert(1)&gt;');
    const report = {
      generatedAt: 'now', windowHours: 1, truncated: false,
      cards: [{ card: 1, role: 'silas', landed: false, lastEventTs: 0, cycleS: 1,
        steps: { workS: null, pushS: null, buildS: null, deployS: null, demoS: null, mergeS: null, finalS: null },
        errors: [{ ts: 0, event: 'x.failed', detail: '</script><script>alert(1)</script>' }] }],
      errorClasses: [{ event: '<b>evil</b>.failed', count: 1 }],
      totals: { cards: 1, landed: 0, withErrors: 1, errorEvents: 1 },
      cycleStats: { landedCards: 0, medianS: null, avgS: null, p90S: null },
    };
    const html = buildHtml(report);
    // the embedded JSON must escape every `<` so </script> breakout is impossible,
    // and the page renders via DOM textContent so the strings stay inert.
    expect(html).not.toContain('</script><script>alert');
    expect(html).not.toContain('<b>evil</b>');
    expect(html).toContain('\\u003c/script>'); // the payload, defanged in the embed (< escaped = no tag can form)
  });
});

describe('deriveWalkAway (#3266) — the walk-away bar', () => {
  const L = (card: number, mins: number, event: string): FlowEvent =>
    ({ ts: T0 + mins * MIN, event, card_id: card, role: 'silas', detail: '' });

  test('clean unattended land extends the streak; manual recovery breaks it', () => {
    const events: FlowEvent[] = [
      // card 1: clean — land run started and landed, accepted inside it
      L(1, 0, 'werk.land.started'), L(1, 5, 'werk.landed'), L(1, 5, 'card.accepted'),
      // card 2: clean
      L(2, 10, 'werk.land.started'), L(2, 15, 'werk.landed'), L(2, 15, 'card.accepted'),
      // card 3: MANUAL RECOVERY — accepted but no werk.landed (the orphan-completion class)
      L(3, 20, 'werk.land.started'), L(3, 21, 'werk.land.failed'), L(3, 30, 'card.accepted'),
      // card 4: clean again — streak restarts after the break
      L(4, 40, 'werk.land.started'), L(4, 45, 'werk.landed'), L(4, 45, 'card.accepted'),
    ];
    const w = deriveWalkAway(events, 3);
    expect(w.k).toBe(3);
    expect(w.currentStreak).toBe(1);          // only card 4 since the card-3 break
    expect(w.lands).toBe(4);                  // accepted cards observed
    expect(w.cleanLands).toBe(3);
    expect(w.ready).toBe(false);
  });

  test('ready flips true only at K consecutive clean lands', () => {
    const events: FlowEvent[] = [];
    for (let i = 1; i <= 5; i++) {
      events.push(L(i, i * 10, 'werk.land.started'), L(i, i * 10 + 5, 'werk.landed'), L(i, i * 10 + 5, 'card.accepted'));
    }
    const w = deriveWalkAway(events, 5);
    expect(w.currentStreak).toBe(5);
    expect(w.ready).toBe(true);
  });

  test('a land.failed whose card still werk.landed later is NOT a streak break (false-red survived)', () => {
    const events: FlowEvent[] = [
      // the tonight-class: merge race emits land.failed, retry lands clean in-run
      L(7, 0, 'werk.land.started'), L(7, 1, 'werk.land.failed'),
      L(7, 5, 'werk.land.started'), L(7, 9, 'werk.landed'), L(7, 9, 'card.accepted'),
    ];
    const w = deriveWalkAway(events, 1);
    // outcome-truth: the card landed via the act path — but it took a RETRY, so it
    // counts as a land, NOT as unattended-clean (someone re-fired it).
    expect(w.lands).toBe(1);
    expect(w.cleanLands).toBe(0);
    expect(w.currentStreak).toBe(0);
    expect(w.ready).toBe(false);
  });

  test('empty events → not-ready, zero streak, never throws', () => {
    const w = deriveWalkAway([], 10);
    expect(w).toMatchObject({ k: 10, lands: 0, cleanLands: 0, currentStreak: 0, ready: false });
  });
});

describe('normalizeLine malformed-ts tolerance (#3266 — the BSD %3N corruption)', () => {
  test('the historical "ts":...N lines parse with the N stripped', () => {
    const e = normalizeLine('{"ts":17811076913N,"event":"werk.landed","card_id":3322,"role":"silas","status":"success"}');
    expect(e?.event).toBe('werk.landed');
    expect(e?.card_id).toBe(3322);
    expect(e?.ts).toBe(17811076913);
  });
});

// --- #3606 — cover normalizeLine branches, esc (were the 40.74% gap).
// (normalizeLine/esc already imported at top of file)

describe('normalizeLine (#3606)', () => {
  it('parses a spine JSON line with numeric ts', () => {
    const e = normalizeLine('{"ts": 1700000000000, "event": "werk.landed", "card_id": 3606, "role": "kade", "reason": "ok"}');
    expect(e).toMatchObject({ ts: 1700000000000, event: 'werk.landed', card_id: 3606, role: 'kade' });
    expect(e!.detail).toBe('ok');
  });

  it('repairs the #3266 malformed-epoch witness lines ("ts":<n>N)', () => {
    const e = normalizeLine('{"ts":1781107691300N,"event":"witness.presented"}');
    expect(e).toMatchObject({ ts: 1781107691300, event: 'witness.presented' });
  });

  it('falls back to timestamp string; rejects unparseable ts', () => {
    expect(normalizeLine('{"timestamp":"2026-07-03T12:00:00Z","event":"x"}')).toMatchObject({ event: 'x' });
    expect(normalizeLine('{"timestamp":"not a date","event":"x"}')).toBeNull();
  });

  it('rejects non-JSON, JSON without event, and non-object lines', () => {
    expect(normalizeLine('plain text')).toBeNull();
    expect(normalizeLine('{"ts": 1}')).toBeNull();
    expect(normalizeLine('{"broken')).toBeNull();
  });

  it('joins detail from the high-signal fields, capped at 200 chars', () => {
    const e = normalizeLine(JSON.stringify({ ts: 1, event: 'e', reason: 'r', error: 'x'.repeat(300) }));
    expect(e!.detail.startsWith('r x')).toBe(true);
    expect(e!.detail.length).toBeLessThanOrEqual(200);
  });
});

describe('esc (#3606)', () => {
  it('escapes all five HTML metacharacters', () => {
    expect(esc(`<a href="x" & 'y'>`)).toBe('&lt;a href=&quot;x&quot; &amp; &#39;y&#39;&gt;');
  });
});
