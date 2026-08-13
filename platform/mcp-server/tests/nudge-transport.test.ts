// @test-type: unit — signal is fixture-data: fake transports, no pulse, no network
//
// #3844 — the facade. Jeff: "nudge becomes a facade for buzz right?"
//
// The AC that matters is not "it still sends." It is that a facade CANNOT turn a
// dead transport into a successful send. That is the failure this whole card
// guards against, and it is the shape that cost a day of Jeff's messages the
// night before: a transport that returned success while delivering the wrong
// thing, with no seam anywhere to notice.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import {
  PulseTransport, sendVia, selectTransports,
  type NudgeTransport, type SendResult, type NudgePayload,
} from '../src/nudge-transport';

const payload: NudgePayload = {
  from: 'wren', to: 'kade', content: 'hi', traceId: 't-1', expects: 'none',
};

class Fake implements NudgeTransport {
  calls = 0;
  constructor(readonly name: string, private result: SendResult) {}
  async send(): Promise<SendResult> { this.calls++; return this.result; }
}

const okT = (n: string) => new Fake(n, { ok: true, resolved: `tmux:%${n}` });
const badT = (n: string) => new Fake(n, { ok: false, error: `${n} is down` });

test('a working transport delivers and names where it landed', async () => {
  const t = okT('pulse');
  const { result } = await sendVia([t], payload);
  assert.equal(result.ok, true);
  if (result.ok) assert.equal(result.resolved, 'tmux:%pulse');
  assert.equal(t.calls, 1);
});

// THE NEGATIVE PROOF (#3734). A facade that swallows a dead transport is worse
// than no facade — "sent" would become a word meaning "we tried."
test('a FAILING transport fails the send — it never reports success', async () => {
  const t = badT('pulse');
  const { result, attempts } = await sendVia([t], payload);
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.match(result.error, /pulse is down/);
    assert.match(result.error, /all transports failed/);
  }
  assert.equal(attempts.length, 1, 'the failure is recorded, not discarded');
});

test('EVERY transport failing is a loud failure naming each one', async () => {
  const { result, attempts } = await sendVia([badT('pulse'), badT('buzz')], payload);
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.match(result.error, /pulse: pulse is down/);
    assert.match(result.error, /buzz: buzz is down/);
  }
  assert.equal(attempts.length, 2);
});

test('fallback: primary down, secondary delivers', async () => {
  const primary = badT('buzz');
  const secondary = okT('pulse');
  const { result, attempts } = await sendVia([primary, secondary], payload);
  assert.equal(result.ok, true);
  assert.equal(primary.calls, 1);
  assert.equal(secondary.calls, 1);
  assert.equal(attempts[0].name, 'buzz', 'the failed attempt is still reported, not hidden by success');
});

test('a working primary does NOT reach the secondary', async () => {
  const primary = okT('buzz');
  const secondary = okT('pulse');
  await sendVia([primary, secondary], payload);
  assert.equal(secondary.calls, 0, 'no double-delivery');
});

// NEGATIVE PROOF: the degenerate config. Empty must fail, not silently no-op —
// a nudge that goes nowhere and reports success is the exact bug class here.
test('no transport configured is a failure, not a quiet success', async () => {
  const { result } = await sendVia([], payload);
  assert.equal(result.ok, false);
  if (!result.ok) assert.match(result.error, /nothing was sent/);
});

test('selection is data: the env list picks order', () => {
  const available = new Map<string, NudgeTransport>([['pulse', okT('pulse')], ['buzz', okT('buzz')]]);
  const { chosen } = selectTransports(available, 'buzz,pulse');
  assert.deepEqual(chosen.map((t) => t.name), ['buzz', 'pulse']);
});

test('default with no env is pulse — today\'s behaviour, unchanged', () => {
  const available = new Map<string, NudgeTransport>([['pulse', okT('pulse')]]);
  const { chosen, unknown } = selectTransports(available, undefined);
  assert.deepEqual(chosen.map((t) => t.name), ['pulse']);
  assert.deepEqual(unknown, []);
});

// NEGATIVE PROOF: a typo must be visible. Silently reverting to the old
// transport is indistinguishable from the change never happening — which is
// how a thing sits broken for two months.
test('an unknown transport name is REPORTED, not silently dropped', () => {
  const available = new Map<string, NudgeTransport>([['pulse', okT('pulse')]]);
  const { chosen, unknown } = selectTransports(available, 'buzzz,pulse');
  assert.deepEqual(unknown, ['buzzz']);
  assert.deepEqual(chosen.map((t) => t.name), ['pulse']);
});

test('PulseTransport surfaces a non-ok status as a failure, with the status in it', async () => {
  const t = new PulseTransport(
    async () => ({ ok: false, status: 503, text: async () => 'upstream down' }),
    'http://localhost:1/api/nudge', undefined, 50,
  );
  const r = await t.send(payload);
  assert.equal(r.ok, false);
  if (!r.ok) assert.match(r.error, /503/);
});

test('PulseTransport returns the destination pulse resolved, not the bare role', async () => {
  const t = new PulseTransport(
    async () => ({ ok: true, status: 200, json: async () => ({ resolved: 'tmux:%21' }) }),
    'http://localhost:1/api/nudge', undefined, 50,
  );
  const r = await t.send(payload);
  assert.equal(r.ok, true);
  if (r.ok) assert.equal(r.resolved, 'tmux:%21');
});

test('a thrown fetch is a failure, not an exception escaping the transport', async () => {
  const t = new PulseTransport(
    async () => { throw new Error('ECONNREFUSED'); },
    'http://localhost:1/api/nudge', undefined, 50,
  );
  const r = await t.send(payload);
  assert.equal(r.ok, false);
  if (!r.ok) assert.match(r.error, /ECONNREFUSED/);
});
