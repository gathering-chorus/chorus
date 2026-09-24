// @test-type: integration — reads and writes a real tmpdir (mkdtemp) the test owns; no store, no crawler, no network
// #4290 — crawler-validate: the control report of git vs graph gaps in both
// directions for every crawler domain. Hermetic: the record directory is a
// tmpdir the test owns (CHORUS_VALIDATE_DIR seam); no store, no crawler.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { readRecords, renderCrawlerValidatePage } from '../src/handlers/crawler-validate';

const RED = {
  ts: '2026-09-24T17:00:00Z', head: 'abc123def456', headTime: '2026-09-24T11:41:42-04:00', watermark: '', crawledAt: '2026-09-24T08:34:10Z',
  rows: [
    { domain: 'code', class: 'CodeFile', tree: 6310, graph: 6310, missing: [], stale: [], measured: true },
    { domain: 'tests', class: 'Test (files)', tree: 1094, graph: 1094,
      missing: ['platform/tests/new-4290.test.sh'], stale: ['platform/tests/gone.bats'],
      excluded: ['platform/services/x/src/mod.rs (contains no test)'], measured: true },
  ],
};
const CLEAN = { ...RED, ts: '2026-09-25T07:00:00Z', rows: RED.rows.map((r) => ({ ...r, missing: [], stale: [] })) };
const UNREAD = { ...RED, ts: '2026-09-26T07:00:00Z', rows: [{ ...RED.rows[0], measured: false }] };

let dir: string;
beforeAll(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'crawler-validate-4290-'));
  fs.writeFileSync(path.join(dir, '2026-09-24T17-00-00Z.json'), JSON.stringify(RED));
  fs.writeFileSync(path.join(dir, '2026-09-25T07-00-00Z.json'), JSON.stringify(CLEAN));
  fs.writeFileSync(path.join(dir, 'latest.json'), JSON.stringify(CLEAN));
  fs.writeFileSync(path.join(dir, 'notes.txt'), 'not a record');
});
afterAll(() => { fs.rmSync(dir, { recursive: true, force: true }); });

describe('#4290 the control report', () => {
  test('reads every kept pass oldest first, skips latest.json and non-records', () => {
    const recs = readRecords(dir);
    expect(recs.map((r) => r.ts)).toEqual(['2026-09-24T17:00:00Z', '2026-09-25T07:00:00Z']);
    expect(recs[0].clean).toBe(false);
    expect(recs[0].gaps).toBe(2);
    expect(recs[1].clean).toBe(true);
  });

  // NEGATIVE PROOF (#3734): the state the page exists to show — one file in
  // git with no row and one row with no file — is red, and BOTH names are on
  // the page under the domain that lacks them.
  test('a missing file and a stale row make the page red and are named', () => {
    const html = renderCrawlerValidatePage(readRecords(dir)[0], readRecords(dir));
    expect(html).toContain('2 gaps between git and the graph');
    expect(html).toContain('platform/tests/new-4290.test.sh');
    expect(html).toContain('platform/tests/gone.bats');
    expect(html).toContain('1 in git, no row');
    expect(html).toContain('1 rows with no source');
    // AC2: an excluded file is listed with its reason, never silent
    expect(html).toContain('1 not counted');
    expect(html).toContain('mod.rs (contains no test)');
    expect(html).toMatch(/<tr class="red"><td>tests<\/td>/);
    expect(html).toMatch(/<tr class="green"><td>code<\/td>/);
  });

  test('every row 0 / 0 and measured is CLEAN, with when it was measured and when main moved', () => {
    const recs = readRecords(dir);
    const html = renderCrawlerValidatePage(recs[1], recs);
    expect(html).toContain('CLEAN — git and the graph hold the same set');
    expect(html).toContain('Measured 2026-09-25T07:00:00Z');
    expect(html).toContain('main last moved 2026-09-24T11:41:42-04:00');
    expect(html).toContain('the crawler last ran 2026-09-24T08:34:10Z');
    expect(html).not.toContain('class="banner red"');
  });

  // NEGATIVE PROOF: a file that claims clean over a row the graph never
  // answered for is NOT clean here — the verdict is derived, never trusted.
  test('an unmeasured row is never clean even when the file says so', () => {
    const d2 = fs.mkdtempSync(path.join(os.tmpdir(), 'crawler-validate-4290-u-'));
    fs.writeFileSync(path.join(d2, 'u.json'), JSON.stringify({ ...UNREAD, clean: true, gaps: 0 }));
    const recs = readRecords(d2);
    expect(recs[0].clean).toBe(false);
    const html = renderCrawlerValidatePage(recs[0], recs);
    expect(html).toContain('unmeasured');
    expect(html).toContain('the graph did not answer for this class');
    expect(html).not.toContain('CLEAN —');
    fs.rmSync(d2, { recursive: true, force: true });
  });

  test('the trend lists past passes newest first with their verdicts', () => {
    const recs = readRecords(dir);
    const html = renderCrawlerValidatePage(recs[1], recs);
    const trend = html.slice(html.indexOf('Past passes'));
    expect(trend.indexOf('2026-09-25T07:00:00Z')).toBeLessThan(trend.indexOf('2026-09-24T17:00:00Z'));
    expect(trend).toContain('2 gaps');
    expect(trend).toContain('>clean<');
  });

  test('no kept pass is an honest empty state, never green', () => {
    expect(readRecords(path.join(dir, 'nowhere'))).toEqual([]);
    const html = renderCrawlerValidatePage(null, []);
    expect(html).toContain('No pass has been kept yet');
    expect(html).not.toContain('CLEAN');
  });
});
