// @test-type: unit — signed fake Jeff posts and a stub pulse on a local port; no live relay.
// @card: #4672
// @owner: wren
// @domain: messages
/**
 * #4672 — each role's copy of a Jeff post is handed to pulse with traceId
 * jeff-<signed id>-<role>, so pulse's Buzz shadow can join his note in #clearing
 * to every row it became (src/jeff-input.ts, src/server.ts). Measured 10-21:
 * his rows carried no trace, so the shadow could not see them at all.
 */
import http from 'http';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { makeInboundHandler } from '../src/server';
import { MessageRouter } from '../src/router';
import { HandoffLog } from '../src/handoff-log';
import { deliverJeffInput, FetchLike } from '../src/jeff-input';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const sk = new Uint8Array(32).fill(84);
const pk = bytesToHex(schnorr.getPublicKey(sk));
const CH = '03e2b251-240f-47fe-89b5-fe4e7ac648fb';

function post(content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['h', CH]];
  const id = buildEventId(serializeEvent(pk, created_at, 9, tags, content));
  return { id, pubkey: pk, created_at, kind: 9, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), sk)) };
}

async function stubPulse(): Promise<{ url: string; got: Array<Record<string, string>>; close: () => Promise<void> }> {
  const got: Array<Record<string, string>> = [];
  const server = http.createServer((req, res) => { let b = ''; req.on('data', (c) => { b += c; }); req.on('end', () => { got.push(JSON.parse(b)); res.writeHead(202).end(); }); });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, got, close: () => new Promise<void>((r) => server.close(() => r())) };
}

test('a post to two roles is handed off twice, each with its own jeff-<id>-<role> trace', async () => {
  const pulse = await stubPulse();
  try {
    const handler = makeInboundHandler({
      router: new MessageRouter(), keys: new Map([[pk, 'jeff']]), pulseUrl: pulse.url,
      broadcastSSE: () => undefined, isRelayConnected: () => true, handoffs: new HandoffLog(undefined, 0),
    });
    const p = post('@kade @silas look');
    handler(p);
    await new Promise((r) => setTimeout(r, 300));
    expect(pulse.got.map((b) => [b.to, b.traceId]).sort()).toEqual([['kade', `jeff-${p.id}-kade`], ['silas', `jeff-${p.id}-silas`]]);
  } finally {
    await pulse.close();
  }
});

test('the trace rides the body; without one the body is unchanged (#4571 shape)', async () => {
  const bodies: string[] = [];
  const f: FetchLike = async (_url, init) => { bodies.push(init.body); return { ok: true, status: 202, text: async () => '' }; };
  await deliverJeffInput('http://pulse', 'kade', 'x', f, 'jeff-abc-kade');
  await deliverJeffInput('http://pulse', 'kade', 'x', f);
  expect(bodies.map((b) => JSON.parse(b))).toEqual([{ to: 'kade', content: 'x', traceId: 'jeff-abc-kade' }, { to: 'kade', content: 'x' }]);
});

test('NEGATIVE PROOF: a retried hand-off carries the same trace, so pulse sees one post, not two', async () => {
  const bodies: string[] = [];
  let n = 0;
  const f: FetchLike = async (_url, init) => {
    bodies.push(init.body);
    if (n++ === 0) throw Object.assign(new Error('fetch failed'), { cause: { code: 'ECONNRESET' } });
    return { ok: true, status: 202, text: async () => '' };
  };
  expect(await deliverJeffInput('http://pulse', 'kade', 'x', f, 'jeff-abc-kade')).toEqual({ ok: true });
  expect(bodies.map((b) => JSON.parse(b).traceId)).toEqual(['jeff-abc-kade', 'jeff-abc-kade']);
});
