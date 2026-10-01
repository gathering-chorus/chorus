// @test-type: unit — drives chorus-api's card read routes in-process (env points at a tempdir); the handlers' own behaviour is covered in tests/handlers
// @card: #4417
// @owner: wren
/**
 * #4417 — the card read routes had no route-level test (Kade's map, 10-01 12:26).
 * Their handlers are covered with injected deps (chorus-card-story 10 cases,
 * athena-card-detail 5, logsForCard in logs-query); this checks the wiring
 * refuses a bad id before anything is asked of Loki or the board.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-4417-'));
process.env.CHORUS_ROOT = TMP;
process.env.DB_PATH = path.join(TMP, 'test.db');
process.env.CHORUS_LOG_PATH = path.join(TMP, 'chorus.log');
process.env.LOKI_URL = 'http://127.0.0.1:9';
// The board and the spine, as fixtures: a stub cards CLI and a temp spine log.
const CARDS = path.join(TMP, 'cards');
fs.writeFileSync(CARDS, `#!/bin/bash
[ "$1" = view ] || exit 1
if [ "$2" = 4417 ]; then
  echo '{"id":4417,"title":"Clearing tests warn before Jeff does","status":"WIP","owner":"Wren","priority":"P1","description":"## AC\\n- [x] no live writes","labels":[]}'
  exit 0
fi
exit 1
`, { mode: 0o755 });
process.env.CHORUS_CARDS_BIN = CARDS;
process.env.CHORUS_SPINE_FILE = path.join(TMP, 'spine.log');
fs.writeFileSync(process.env.CHORUS_SPINE_FILE, JSON.stringify({ timestamp: new Date().toISOString(), event: 'card.pulled', role: 'wren', card: '4417' }) + '\n');

import app from '../src/server';
import type { AddressInfo } from 'net';

let base = '';
const srv = app.listen(0);
beforeAll(async () => {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  base = `http://127.0.0.1:${(srv.address() as AddressInfo).port}`;
});
afterAll(() => new Promise<void>((r) => srv.close(() => r())));

describe('#4417 card read routes refuse a bad id', () => {
  test.each(['/api/chorus/pain/card/abc', '/api/chorus/logs/card/abc'])('NEGATIVE PROOF: %s → 400 bad-card-id', async (p) => {
    const res = await fetch(`${base}${p}`);
    expect(res.status).toBe(400);
    expect((await res.json()).reason).toBe('bad-card-id');
  });

  test('a good id with Loki unreachable → 502, never a 200 with nothing in it', async () => {
    const res = await fetch(`${base}/api/chorus/logs/card/4417`);
    expect(res.status).toBe(502);
  });
});

describe('#4417 the card story and the card detail', () => {
  test('card-story reads the board and the spine it is given, not the live ones', async () => {
    const res = await fetch(`${base}/api/chorus/card-story/4417`);
    expect(res.status).toBe(200);
    expect(JSON.stringify(await res.json())).toContain('Clearing tests warn before Jeff does');
  });

  test('NEGATIVE PROOF: a non-numeric id → 400', async () => {
    const res = await fetch(`${base}/api/chorus/card-story/abc`);
    expect(res.status).toBe(400);
  });
  // Not pinned: an id the board does not know answers 200 with an empty story
  // (each source is optional by design, chorus-card-story.ts:12). Recorded on #4417.

  test('athena/card answers the card the board returns', async () => {
    const res = await fetch(`${base}/api/athena/card/4417`);
    expect(res.status).toBe(200);
    expect(JSON.stringify(await res.json())).toContain('Clearing tests warn before Jeff does');
  });
});
