// @test-type: unit — drives deliverJeffMessageToTarget against a stub pulse on an ephemeral port and a stub chorus-log; brings its own world.
// @card: #3343
// @owner: wren
/**
 * #3343 — Jeff's Clearing input rides the pulse delivery worker.
 *
 * #4417 — this file used to read server.ts as text and match strings. It now
 * runs the hand-off: a stub pulse records what the Clearing sends, and a stub
 * chorus-log under a temp CHORUS_ROOT records the audit events. The ack and
 * processJeffInput wiring are run-tested in jeff-input-ack.test.ts.
 */
import * as fs from 'fs';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import type { AddressInfo } from 'net';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'jeff-input-delivery-'));
const AUDIT = path.join(TMP, 'audit.log');
fs.mkdirSync(path.join(TMP, 'platform', 'scripts'), { recursive: true });
fs.writeFileSync(path.join(TMP, 'platform', 'scripts', 'chorus-log'), `#!/bin/sh\necho "$*" >> "${AUDIT}"\n`, { mode: 0o755 });

interface Seen { method: string; url: string; caller: string | undefined; body: { to: string; content: string } }
const seen: Seen[] = [];
let answer = 200;
const pulse = http.createServer((req, res) => {
  let raw = '';
  req.on('data', (c) => { raw += c; });
  req.on('end', () => {
    seen.push({ method: req.method || '', url: req.url || '', caller: req.headers['x-chorus-clearing-caller'] as string | undefined, body: JSON.parse(raw || '{}') });
    res.statusCode = answer;
    res.end(answer === 200 ? '{"ok":true}' : '{"error":"stub refused"}');
  });
});

let deliver: (target: string, safeMsg: string, cleanText: string) => Promise<string | null>;
let srv: { server: http.Server; io: { close: () => void } };

beforeAll(async () => {
  await new Promise<void>((r) => pulse.listen(0, '127.0.0.1', () => r()));
  process.env.PULSE_URL = `http://127.0.0.1:${(pulse.address() as AddressInfo).port}`;
  process.env.CHORUS_ROOT = TMP;
  process.env.CLEARING_SCAN_DIR = TMP;
  process.env.CLEARING_PROJECTS_DIR = TMP;
  process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');
  srv = require('../src/server');
  deliver = require('../src/server').deliverJeffMessageToTarget;
});

afterAll(async () => {
  srv?.io.close();
  await new Promise<void>((r) => pulse.close(() => r()));
  fs.rmSync(TMP, { recursive: true, force: true });
});

beforeEach(() => { seen.length = 0; answer = 200; try { fs.unlinkSync(AUDIT); } catch { /* none yet */ } });

const audit = async (): Promise<string> => {
  for (let i = 0; i < 20; i++) {
    if (fs.existsSync(AUDIT)) return fs.readFileSync(AUDIT, 'utf8');
    await new Promise((r) => setTimeout(r, 25));
  }
  return '';
};

describe('#3343 Jeff\'s input is handed to pulse as he typed it', () => {
  test('one POST to /api/jeff-input, Clearing caller header, content raw', async () => {
    expect(await deliver('wren', 'hello [x]', 'hello [x]')).toBeNull();
    expect(seen).toEqual([{
      method: 'POST',
      url: '/api/jeff-input',
      caller: '1',
      body: { to: 'wren', content: 'hello [x]' },
    }]);
    expect(seen[0].body.content).not.toMatch(/\[nudge from/);
    expect(await audit()).toMatch(/^jeff\.input\.delivered bridge to=wren chars=9$/m);
  });

  test('pulse refuses → the reason comes back and jeff.input.failed is logged', async () => {
    answer = 500;
    const reason = await deliver('kade', 'go', 'go');
    expect(reason).toMatch(/^pulse 500/);
    const log = await audit();
    expect(log).toMatch(/^jeff\.input\.failed bridge to=kade chars=2 reason=pulse 500/m);
    // NEGATIVE PROOF: a refused hand-off is never logged as delivered.
    expect(log).not.toMatch(/jeff\.input\.delivered/);
  });
});
