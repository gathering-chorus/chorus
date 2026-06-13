/**
 * #3097 blocker 1 — chorus-api soft-fails when the Gathering repo is absent.
 *
 * The extraction needs chorus-api to survive gathering being moved or removed:
 * the readers of GATHERING_ROOT/GATHERING_REPO must skip cleanly (empty result,
 * no throw, no degraded health) when the path doesn't exist — not break the API.
 *
 * These pin the soft-fail at the gathering-scan seams (the page/endpoint
 * discovery readers). They are hermetic: pure functions against a path that
 * cannot exist, no live ~/.chorus state, no running service. Mirrors the
 * already-tested scanLoomHtml('/nonexistent') contract.
 */

import { scanEjsViews, scanDocHtml } from '../src/discover-pages-gathering';

const ABSENT = '/nonexistent-gathering-root-3097';

describe('#3097 — gathering-absent soft-fail at the scan seams', () => {
  test('scanEjsViews returns [] when the gathering views dir is absent (no throw)', () => {
    expect(scanEjsViews(`${ABSENT}/views`, {})).toEqual([]);
  });

  test('scanDocHtml returns [] when the gathering-docs dir is absent (no throw)', () => {
    expect(scanDocHtml(`${ABSENT}/public/gathering-docs`, {})).toEqual([]);
  });

  test('the scan seams never throw on an absent root — they degrade to empty', () => {
    // The blocker is "chorus-api breaks if gathering is removed"; the contract
    // is that an absent root is a skipped scan, never a thrown error that would
    // 500 the discover endpoints or degrade health.
    expect(() => scanEjsViews(`${ABSENT}/views`, { foo: 'bar' })).not.toThrow();
    expect(() => scanDocHtml(`${ABSENT}/public/gathering-docs`, { foo: 'bar' })).not.toThrow();
  });
});
