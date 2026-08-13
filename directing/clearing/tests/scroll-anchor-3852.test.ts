// @test-type: unit — signal is fixture-data: reads the served client source, no browser
//
// #3852 — Jeff, 2026-08-13: "if i scroll up i get forced back to bottom even
// when no new messages arrive... message arrive must not force to bottom."
//
// The client is inline in index.html, so there is no module to import. What CAN
// be checked without a browser is the shape: that arrival paths are guarded and
// that the position is sampled BEFORE the DOM grows — the ordering bug that
// makes a correct-looking guard always report un-pinned.
import * as fs from 'fs';
import * as path from 'path';

const SRC = fs.readFileSync(path.join(__dirname, '../public/index.html'), 'utf8');

/** The message-arrival handler body. */
function arrivalHandler(): string {
  const i = SRC.indexOf("socket.on('message'");
  expect(i).toBeGreaterThan(-1);
  return SRC.slice(i, i + 400);
}

describe('#3852 arrival must not steal scroll', () => {
  it('the arrival handler does NOT call scrollToBottom unconditionally', () => {
    const h = arrivalHandler();
    // NEGATIVE PROOF: this exact line was the bug. If it comes back, so does
    // the defect, and this test is the thing that says so.
    expect(h).not.toMatch(/renderTranscript\(\);\s*\n\s*scrollToBottom\(\);/);
  });

  it('the arrival handler guards on pinned state', () => {
    expect(arrivalHandler()).toMatch(/scrollToBottomIfPinned/);
  });

  it('position is sampled BEFORE render — after it, scrollHeight has already grown', () => {
    const h = arrivalHandler();
    const sample = h.indexOf('isPinnedToBottom()');
    const render = h.indexOf('renderTranscript()');
    expect(sample).toBeGreaterThan(-1);
    expect(render).toBeGreaterThan(-1);
    expect(sample).toBeLessThan(render);
  });

  it('a deliberate scroll-to-bottom still exists for first load and own sends', () => {
    // Over-correcting to "never scroll" is the opposite bug: the room would
    // open scrolled up, and your own message would not appear.
    expect(SRC).toMatch(/function scrollToBottom\(\)/);
    expect(SRC).toMatch(/socket\.on\('messages'[\s\S]{0,200}scrollToBottom\(\)/);
  });

  it('the mobile stream follows the same rule', () => {
    expect(SRC).toMatch(/streamWasPinned/);
    expect(SRC).not.toMatch(/\}\)\.join\(''\);\s*\n\s*el\.scrollTop = el\.scrollHeight;/);
  });
});
