// @test-type: fitness — reads the test sources; no live services
// @domain: messages
// @card: #4571
// @owner: wren
// #4571 — Jeff, 20:15: "Hello team from Jeff browser" reached Wren's terminal three times
// and "not me must be code or test". server.test.ts built apps on prod pulse (:3475), and once
// Jeff's room messages were handed to /api/jeff-input, each test run typed into a live role.
// No test may build an app, tile service or stub that talks to the live pulse.
import fs from 'fs';
import path from 'path';

const LIVE = /(pulseUrl:\s*|TileService\()['"`]https?:\/\/(127\.0\.0\.1|localhost):3475/;

export function liveCallers(dir: string): string[] {
  return fs.readdirSync(dir).filter((f) => f.endsWith('.test.ts'))
    .flatMap((f) => fs.readFileSync(path.join(dir, f), 'utf8').split('\n')
      .map((l, i) => (LIVE.test(l) ? `${f}:${i + 1}` : '')).filter(Boolean));
}

test('no test points createApp or TileService at the live pulse', () => {
  expect(liveCallers(__dirname)).toEqual([]);
});

test('negative proof: a test file that does is caught', () => {
  const dir = fs.mkdtempSync(path.join(require('os').tmpdir(), 'v2-live-'));
  try {
    fs.writeFileSync(path.join(dir, 'x.test.ts'), "createApp({ pulseUrl: 'http://127.0.0.1:" + '3475' + "' });\n");
    expect(liveCallers(dir)).toEqual(['x.test.ts:1']);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
