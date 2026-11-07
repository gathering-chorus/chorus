// @test-type: unit — fixture readings rendered to HTML; no live store.
// @card: #4647
// @owner: kade
// @domain: heralds
/**
 * #4647 — /sync shows git vs graph per crawled class from the sync sensor's
 * readings: a summary first, the newest day's classes worst first, then the
 * history (Wren 18:39: its own page, reach don't look, 390 px). Above 0 is red;
 * a class or a day with no reading says unmeasured, never 0.
 */
import { syncByDay } from '../src/sources/readings';
import { renderSyncPage } from '../src/views';

const r = (subject: string, metric: string, day: string, value: number) => ({ readingOfSensor: 'sensor-sync', readingSubject: subject, readingMetric: metric, readingDay: day, readingValue: String(value) });
// the first live dry run, 10-19 18:44
const live = ['codefile', 'test-files', 'test-cases', 'logsource', 'page', 'endpoint'].flatMap((s) => [r(s, 'missing', '2026-10-19', 0), r(s, 'stale', '2026-10-19', 0)])
  .concat([r('test-declared-domain', 'missing', '2026-10-19', 3), r('test-declared-domain', 'stale', '2026-10-19', 0)]);

describe('#4647 /sync', () => {
  test('the worst class comes first and the summary names the gap', () => {
    const days = syncByDay(live);
    expect(days[0].classes[0]).toEqual({ subject: 'test-declared-domain', missing: 3, stale: 0 });
    const html = renderSyncPage(days, {}, []);
    expect(html).toContain('2026-10-19: 1 of 7 classes out of sync</b>, 3 missing, 0 stale');
    expect(html).toMatch(/<td>test-declared-domain<\/td><td class="num fail">3<\/td>/);
    expect(html).toMatch(/<td>codefile<\/td><td class="num ">0<\/td>/);
  });

  test('a day with every class at 0 reads in sync, and earlier days list only what was out', () => {
    const days = syncByDay([...live, r('codefile', 'missing', '2026-10-20', 0), r('codefile', 'stale', '2026-10-20', 0)]);
    const html = renderSyncPage(days, {}, []);
    expect(html).toContain('2026-10-20: all 1 classes in sync');
    expect(html).toMatch(/<td>10-19<\/td><td class="fail">test-declared-domain<\/td>/);
  });

  test('other sensors and role readings are not sync rows', () => {
    expect(syncByDay([{ readingOfSensor: 'sensor-messages', readingMetric: 'missing', readingDay: '2026-10-19', readingValue: '1', readingSubject: 'x' }])).toEqual([]);
  });

  // NEGATIVE PROOF: a missing metric or no readings at all is unmeasured, never a reassuring 0
  test('negative proof: half a class or no readings is unmeasured, never in sync', () => {
    const half = renderSyncPage(syncByDay([r('codefile', 'missing', '2026-10-19', 0)]), {}, []);
    expect(half).toContain('2026-10-19: unmeasured</b> for codefile');
    expect(half).not.toContain('in sync');
    const none = renderSyncPage([], {}, []);
    expect(none).toContain('Unmeasured: the sync sensor has written no readings yet');
    const err = renderSyncPage([], { error: 'the readings door answered 503' }, []);
    expect(err).toContain('Unmeasured: the readings door answered 503');
  });
});
