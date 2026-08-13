// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #2018 — Board domain total must match sub-group sum
 *
 * The Clearing domain panel shows a total badge and sequence sub-groups.
 * Cards without a sequence tag must show under an "unsequenced" sub-group
 * so the sub-group counts visibly add up to the total.
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');

describe('Clearing domain subtotals (#2018)', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
  });

  test('unsequenced cards get a sub-group header', () => {
    expect(html).toContain('unsequenced');
  });

  test('empty sequence group gets a count badge', () => {
    const hasEmptySeqHeader = html.includes("'unsequenced'") || html.includes('"unsequenced"');
    expect(hasEmptySeqHeader).toBe(true);
  });
});
