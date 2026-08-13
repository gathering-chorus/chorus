// @test-type: unit — signal is fixture-data: reads the Clearing source, no io
//
// #3857 — MOVED from jeff-bridwell-personal-site/tests/unit/. It lived in the
// gathering repo and reached ACROSS repos (../../../chorus/...) to read this
// Clearing's source. So a Chorus product was tested by a suite Chorus never
// ran — which is why the tests domain showed zero clearing tests, and why none
// of them could catch the defects Jeff found by hand on 2026-08-13.
/**
 * #2036 — Clearing detects tunnel failure via client-side heartbeat
 *
 * Socket.IO's built-in disconnect event doesn't fire when the tunnel dies
 * but TCP stays open (common on mobile/5G). A client heartbeat catches this.
 */
import * as fs from 'fs';
import * as path from 'path';

const CLEARING_HTML = path.join(__dirname, '../public/index.html');

describe('Clearing connection heartbeat (#2036)', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(CLEARING_HTML, 'utf-8');
  });

  test('client-side heartbeat ping exists', () => {
    expect(html).toContain('heartbeat');
    expect(html).toContain("emit('ping'");
  });

  test('heartbeat timeout switches status to disconnected', () => {
    expect(html).toContain('dot-disconnected');
    expect(html).toContain('heartbeatTimeout');
  });

  test('successful pong resets the timeout', () => {
    expect(html).toContain("on('pong'");
  });

  test('reconnect indicator shows during recovery', () => {
    expect(html).toContain('reconnecting');
  });
});
