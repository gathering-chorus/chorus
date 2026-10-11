// @test-type: unit
// @domain: tests · @card: 4253 · owner: kade
// #4253 — Jeff, 2026-09-21 08:22, reading the nightly page: "on the report i
// cant tell timing on runs" — then "each suit". Every suite row shows its
// seconds, every timed suite can be read slowest first, and a suite the run
// did not time reads as unknown, never as 0 s.
import { parseNightlyLog, renderNightlyPage, secsCell } from '../src/handlers/nightly-report';

const LOG = [
  'RUN|start|2026-10-11T03:00:00',
  'SUITE|bats|platform/tests/a.bats|kade|pass|3 pass, 0 fail',
  'SUITETIME|bats|platform/tests/a.bats|531.0',
  'SUITE|bats|platform/tests/b.bats|wren|fail|0 pass, 1 fail',
  'SUITETIME|bats|platform/tests/b.bats|12.5',
  'SUITE|cargo|platform/services/old|silas|pass|9 pass, 0 fail',
  'RUN|complete|2026-10-11T04:43:00',
].join('\n');

describe('#4253 each suite shows how long it took', () => {
  it('the log parser attaches each SUITETIME to its own row', () => {
    const run = parseNightlyLog(LOG)!;
    expect(run.rows.map((r) => [r.path, r.seconds])).toEqual([
      ['platform/tests/a.bats', 531], ['platform/tests/b.bats', 12.5], ['platform/services/old', undefined]]);
  });

  it('every suite row on the page carries its time, a red one too', () => {
    const html = renderNightlyPage(parseNightlyLog(LOG));
    expect(html).toContain('531.0 s');
    expect(html).toMatch(/pill red">fail<\/span><span class="kind">bats<\/span><span class="secs">12\.5 s/);
  });

  it('NEGATIVE PROOF: a row with no time renders as unknown, never 0 s', () => {
    expect(secsCell({})).toContain('—');
    expect(secsCell({})).not.toContain('0 s');
    expect(renderNightlyPage(parseNightlyLog(LOG))).not.toContain('>0.0 s<');
  });

  it('with more than ten timed suites, all of them read slowest first', () => {
    const many = ['RUN|start|x', ...Array.from({ length: 12 }, (_, i) => [
      `SUITE|bats|platform/tests/s${i}.bats|kade|pass|1 pass, 0 fail`, `SUITETIME|bats|platform/tests/s${i}.bats|${i + 1}.0`]).flat(), 'RUN|complete|y'].join('\n');
    const html = renderNightlyPage(parseNightlyLog(many));
    expect(html).toContain('all 12 timed suites, slowest first');
    const all = html.slice(html.indexOf('all 12 timed suites'));
    expect(all.indexOf('12.0 s')).toBeLessThan(all.indexOf('1.0 s'));
  });
});
