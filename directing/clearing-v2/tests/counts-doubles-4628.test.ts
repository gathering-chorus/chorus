// @test-type: unit — fixture readings rendered to HTML; no live store.
// @card: #4628
// @owner: silas
// @domain: messages
/**
 * #4628 — /counts shows doubles in Buzz per day per kind, from the sensor's
 * doubles-<kind> readings, in their own short table so it reads at phone width
 * (Wren: 390 px, no sideways scroll). Above 0 is red: one writer per kind
 * means a double is a second writer left in place.
 */
import { byDay, DOUBLE_COLUMNS } from '../src/sources/readings';
import { renderCountsPage } from '../src/views';

const r = (metric: string, day: string, value: number) => ({ readingOfSensor: 'sensor-messages', readingMetric: metric, readingDay: day, readingValue: String(value) });
const sent = r('messages-sent', '2026-10-19', 395);

describe('#4628 /counts shows doubles per kind', () => {
  test('one column per kind, labelled for Jeff', () => {
    expect(DOUBLE_COLUMNS.map(([, m]) => m)).toEqual(['doubles-jeff', 'doubles-reply', 'doubles-nudge', 'doubles-alert', 'doubles-land']);
    expect(DOUBLE_COLUMNS.map(([, , l]) => l)).toEqual(['Yours', 'Replies', 'Nudges', 'Alerts', 'Lands']);
  });

  test('10-19 as measured: yours and replies red, the rest 0 and not red', () => {
    const doubles = byDay([r('doubles-jeff', '2026-10-19', 93), r('doubles-reply', '2026-10-19', 413), r('doubles-nudge', '2026-10-19', 0), r('doubles-alert', '2026-10-19', 0), r('doubles-land', '2026-10-19', 0)], DOUBLE_COLUMNS);
    const html = renderCountsPage(byDay([sent]), {}, [], doubles);
    expect(html).toContain('Doubles in Buzz');
    expect(html).toMatch(/<td class="num fail">93<\/td>/);
    expect(html).toMatch(/<td class="num fail">413<\/td>/);
    expect(html).toMatch(/<td class="num ">0<\/td>/);
  });

  // NEGATIVE PROOF: a day before the sensor counted doubles is unmeasured, never a reassuring 0
  test('negative proof: a day with no doubles reading says unmeasured, and no doubles table means none was measured', () => {
    const doubles = byDay([r('doubles-jeff', '2026-10-19', 0)], DOUBLE_COLUMNS);
    const html = renderCountsPage(byDay([sent]), {}, [], doubles);
    expect(html).toMatch(/Doubles in Buzz[\s\S]*unmeasured/);
    const none = renderCountsPage(byDay([sent]), {}, [], []);
    expect(none).not.toContain('Doubles in Buzz');
    expect(none).toContain('Doubles: unmeasured');
  });

  test('the doubles table is six short columns, so it fits a phone', () => {
    const doubles = byDay([r('doubles-jeff', '2026-10-19', 1)], DOUBLE_COLUMNS);
    const html = renderCountsPage(byDay([sent]), {}, [], doubles);
    const table = html.slice(html.indexOf('Doubles in Buzz'));
    const head = table.slice(table.indexOf('<thead>'), table.indexOf('</thead>'));
    expect((head.match(/<th[ >]/g) || []).length).toBe(6); // header cells, not the <thead> tag
  });
});
