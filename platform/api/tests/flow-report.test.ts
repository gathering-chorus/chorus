/**
 * flow-report core — unit tests (#3269).
 *
 * The card cycle/step/error fitness function as a pure aggregation: spine
 * events in → structured JSON out (per-card cycle, step times, errors as
 * children, ranked error classes). The instrument behind #3266's walk-away
 * bar, replacing the 06-06 one-off bash→Loki→HTML report.
 */
import { aggregateFlow, FlowEvent } from '../src/flow-report';
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
