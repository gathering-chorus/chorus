// @test-type: unit — signal is fixture-data: fake fetch + CHORUS_LOG_FILE in a tempdir, no live pulse and no real spine
// #3818 — the cap wired into the send path.
//
// These test the PLACEMENT, not the counting (counting lives in
// word-cap.test.ts against the shared fixture). Placement is where this gate
// can go wrong in ways the counting tests cannot see.
//
// The test brings its own world (#3528): CHORUS_LOG_FILE points into a tempdir
// so nothing here touches the real spine.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'wordcap-3818-'));
process.env.CHORUS_LOG_FILE = path.join(tmp, 'spine.log');
process.env.CHORUS_PULSE_URL = 'http://127.0.0.1:9/api/nudge'; // never reached in a bounce

import { executeNudge } from '../src/server';
import { __clearCapCache } from '../src/word-cap';

const spine = (): string =>
  fs.existsSync(process.env.CHORUS_LOG_FILE!) ? fs.readFileSync(process.env.CHORUS_LOG_FILE!, 'utf8') : '';

/** A fetch that answers the cap lookup and records every other call. */
function fakeFetch(cap: number, sends: string[]) {
  return async (url: string, init?: unknown) => {
    if (String(url).includes('properties/resolve')) {
      return { ok: true, status: 200, json: async () => ({ value: cap }), text: async () => '' };
    }
    sends.push(String(url));
    return { ok: true, status: 200, json: async () => ({ resolved: 'tmux:%1' }), text: async () => '' };
  };
}

const words = (n: number) => Array.from({ length: n }, (_, i) => `w${i}`).join(' ');

test('an at-cap message sends normally', async () => {
  __clearCapCache();
  const sends: string[] = [];
  const res = await executeNudge(
    { to: 'kade', message: words(5) } as never, 'wren',
    fakeFetch(5, sends) as never,
  );
  assert.match(res.content[0].text, /nudge sent/);
  assert.equal(sends.length, 1, 'the pulse POST happened');
});

// NEGATIVE PROOF (#3734): the gate must actually refuse.
test('an over-cap message is REJECTED and never reaches the transport', async () => {
  __clearCapCache();
  const sends: string[] = [];
  const res = await executeNudge(
    { to: 'kade', message: words(9) } as never, 'wren',
    fakeFetch(5, sends) as never,
  );
  assert.match(res.content[0].text, /word-cap: 9 words, cap is 5/);
  assert.equal(sends.length, 0, 'nothing was sent');
});

// The placement test. The spine emits fire BEFORE the pulse POST by design (so
// the audit trail survives an unreachable pulse) — which means a gate placed
// after them would record a message as requested+emitted that was never sent.
test('a rejected message leaves NO nudge.requested / nudge.emitted behind', async () => {
  __clearCapCache();
  fs.writeFileSync(process.env.CHORUS_LOG_FILE!, '');
  await executeNudge(
    { to: 'kade', message: words(40) } as never, 'wren',
    fakeFetch(5, []) as never,
  );
  const log = spine();
  assert.ok(!log.includes('nudge.requested'), 'no requested event for a message that was never sent');
  assert.ok(!log.includes('nudge.emitted'), 'no emitted event either');
});

// Kade's deadlock catch. RESPOND-FIRST blocks a role's tools until it answers a
// peer. If an over-cap reply bounced AND consumed the obligation, the role would
// be free without the peer ever hearing — worse than the deadlock. If it bounced
// and wedged, the role could never recover. It must do neither: bounce, change
// nothing, stay callable, and let a shorter resend through.
test('a bounce is retryable — a shorter resend goes through immediately', async () => {
  __clearCapCache();
  const sends: string[] = [];
  const f = fakeFetch(5, sends) as never;

  const bounced = await executeNudge({ to: 'kade', message: words(20) } as never, 'wren', f);
  assert.match(bounced.content[0].text, /word-cap/);
  assert.equal(sends.length, 0);

  const retried = await executeNudge({ to: 'kade', message: words(4) } as never, 'wren', f);
  assert.match(retried.content[0].text, /nudge sent/);
  assert.equal(sends.length, 1, 'the shorter resend reached the transport');
});

// Fail-open at the wired level, not just in the resolver: if the cap lookup
// itself throws, the message must still go. A dead store must never mute us.
test('a cap lookup that throws still sends the message', async () => {
  __clearCapCache();
  const sends: string[] = [];
  const f = (async (url: string) => {
    if (String(url).includes('properties/resolve')) throw new Error('ECONNREFUSED');
    sends.push(String(url));
    return { ok: true, status: 200, json: async () => ({}), text: async () => '' };
  }) as never;
  const res = await executeNudge({ to: 'kade', message: words(30) } as never, 'wren', f);
  assert.match(res.content[0].text, /nudge sent/, 'fail-open: 30 words < default 100');
  assert.equal(sends.length, 1);
});
