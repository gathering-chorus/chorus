// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #2046 — Clearing mobile UX: Enter sends instead of newline, focus stolen by re-renders
 *
 * AC:
 * 1. Mobile: Enter = newline, Send button = send
 * 2. Focus not stolen from input during tile/message refreshes
 * 3. Send only on explicit Send button tap on mobile
 * 4. Tests cover: mobile input retains focus after refresh, newline insertion works
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');

describe('Clearing mobile UX (#2046)', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
  });

  test('AC1+AC3: input is a textarea for multiline support', () => {
    expect(html).toMatch(/<textarea[^>]*class="input-field"[^>]*id="input"/);
  });

  test('AC1+AC3: Enter key is gated on mobile — no send on mobile Enter', () => {
    // Mobile check: isMobile() returns early without sending
    expect(html).toContain('isMobile()');
    expect(html).toMatch(/if\s*\(isMobile\(\)\)/);
    // Desktop: Enter without shift sends
    expect(html).toContain('!e.shiftKey');
  });

  test('AC2: toggleLock focus gated on desktop only', () => {
    expect(html).toContain('if (!isMobile()) input.focus()');
  });

  test('AC2: page-load focus skipped on mobile', () => {
    // The initial focus call should be gated
    const lines = html.split('\n');
    const focusOnLoad = lines.find(l => l.includes('Focus input on load'));
    expect(focusOnLoad).toBeDefined();
    const nextLine = lines[lines.indexOf(focusOnLoad!) + 1];
    expect(nextLine).toContain('isMobile()');
  });

  test('AC1: textarea auto-resizes on input', () => {
    expect(html).toContain("input.addEventListener('input'");
    expect(html).toContain('input.scrollHeight');
  });

  test('AC3: mobile hint says tap Send, desktop hint says Enter', () => {
    expect(html).toContain('class="mobile-hint"');
    expect(html).toContain('class="desktop-hint"');
    expect(html).toMatch(/mobile-hint.*tap Send/);
    expect(html).toMatch(/desktop-hint.*Enter to send/);
  });
});
