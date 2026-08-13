// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #2034 — Clearing streams pane fills viewport on mobile
 *
 * The mobile-stream div must use absolute positioning to fill the viewport
 * when the streams tab is active, matching the pattern used by domains and messages.
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');

describe('Clearing mobile streams (#2034)', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
  });

  test('streams tab uses fixed positioning with bottom offset for tabs+input', () => {
    expect(html).toContain("mobileStream.style.position = 'fixed'");
    expect(html).toContain("mobileStream.style.top = '0'");
    expect(html).toContain("(tabsH + inputH) + 'px'");
  });

  test('mobile-stream div has background color to cover content beneath', () => {
    expect(html).toContain('background:#0d1117');
  });
});
