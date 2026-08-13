// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #2052 — Clearing domain badge counts must respect role filter
 *
 * When a role filter is active, the domain header badge should show
 * filtered card counts (cards.length), not unfiltered API counts
 * (counts.activeCards / counts.activeTotal).
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');

describe('Clearing role filter badge (#2052)', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
  });

  test('cardBadge uses filtered count when role filter is active', () => {
    // The fix gates cardBadge on activeRoleFilter !== 'all'
    // When filtered, it should use cards.length, not counts.activeCards
    const hasFilteredCardBadge = html.includes("activeRoleFilter !== 'all'") &&
      html.includes('cards.length');
    expect(hasFilteredCardBadge).toBe(true);
  });

  test('totalBadge uses filtered count when role filter is active', () => {
    // totalBadge should also use cards.length when filtered
    const lines = html.split('\n');
    const totalBadgeLine = lines.find(l => l.includes('totalBadge') && l.includes('total'));
    expect(totalBadgeLine).toBeDefined();
    // Should reference filtered variable or cards.length, not just counts.activeTotal
    expect(totalBadgeLine).toMatch(/filtered.*cards\.length|cards\.length.*filtered/);
  });

  test('sub-group counts sum to header when filtered', () => {
    // The cards array that gets split into bySequence is the same filtered array
    // used for the header badge, so they are automatically consistent.
    // Verify the role filter happens before both badge and sub-group rendering.
    const filterIndex = html.indexOf("activeRoleFilter !== 'all'");
    const bySequenceIndex = html.indexOf('bySequence');
    // Filter check must appear before sub-group grouping
    expect(filterIndex).toBeGreaterThan(-1);
    expect(bySequenceIndex).toBeGreaterThan(filterIndex);
  });
});
