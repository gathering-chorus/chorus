// @test-type: integration — a stub relay on a local socket stands in for Buzz's #team channel; no live relay.
// @domain: messages
// @card: #4652
// @owner: silas
/**
 * #4652 — the Buzz app's #team shows only channel messages: kind 9 with an `h`
 * tag naming the channel's id (measured 10-20 06:53 in the relay's own tables:
 * #team is 160e1f5d-…, every message there is kind 9). Pulse wrote kind 1 with
 * a `t=team` topic, which the relay stored and the app never showed.
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { BuzzWriter, type BuzzNote } from './buzz-writer';

const TEAM = '160e1f5d-6816-4c68-94b3-b5f1ade75565';
const sk = new Uint8Array(32).fill(62);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

/** A relay that behaves like #team: only a kind 9 naming `channel` is a channel message. */
async function teamRelay(channel: string) {
  const got: BuzzNote[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (data) => {
    const f = JSON.parse((data as Buffer).toString('utf8'));
    if (f[0] !== 'EVENT') return;
    const n = f[1] as BuzzNote;
    got.push(n);
    const inChannel = n.kind === 9 && n.tags.some((t) => t[0] === 'h' && t[1] === channel);
    ws.send(JSON.stringify(['OK', n.id, inChannel, inChannel ? '' : 'test relay: not a #team channel message']));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
  const close = () => new Promise<void>((r) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => r())); });
  return { url, got, close };
}

const until = async (cond: () => boolean, ms = 3000) => {
  for (let t = 0; t < ms && !cond(); t += 20) await new Promise((r) => setTimeout(r, 20));
};

async function postAll(channel: string, posts: Parameters<BuzzWriter['post']>[0][]) {
  const relay = await teamRelay(channel);
  const onMiss = jest.fn();
  const w = new BuzzWriter({ relayUrl: relay.url, signer: bridge, okTimeoutMs: 2000, onMiss });
  try {
    await w.ready();
    for (const p of posts) w.post(p);
    await until(() => relay.got.length === posts.length);
    await new Promise((r) => setTimeout(r, 150));
    return { got: relay.got, onMiss };
  } finally { w.close(); await relay.close(); }
}

describe('#4652 pulse posts into the app\'s #team channel', () => {
  afterEach(() => { delete process.env.BUZZ_TEAM_CHANNEL; });

  test('by default a nudge, a terminal line and a land notice are each one kind 9 in #team, keeping author, kind and id', async () => {
    const { got, onMiss } = await postAll(TEAM, [
      { id: 1, from: 'kade', to: 'silas', content: 'hi' },
      { id: 2, from: 'jeff', to: 'wren', content: 'typed', surface: 'terminal' },
      { id: 4652, from: 'jeff', to: 'wren', content: 'Accepted #4652', surface: 'land' },
    ]);
    expect(got.map((n) => n.kind)).toEqual([9, 9, 9]);
    for (const n of got) expect(n.tags).toEqual(expect.arrayContaining([['h', TEAM], ['t', 'team']]));
    expect(got[0].tags).toEqual(expect.arrayContaining([['author', 'kade'], ['kind', 'nudge'], ['msg', 'pulse-1']]));
    expect(got[1].tags).toEqual(expect.arrayContaining([['author', 'jeff'], ['kind', 'jeff'], ['msg', 'pulse-term-2']]));
    expect(got[2].tags).toEqual(expect.arrayContaining([['author', 'jeff'], ['kind', 'land'], ['msg', 'pulse-land-4652']]));
    expect(onMiss).not.toHaveBeenCalled();
  });

  test('BUZZ_TEAM_CHANNEL names another channel', async () => {
    process.env.BUZZ_TEAM_CHANNEL = 'aaaaaaaa-0000-4000-8000-000000000001';
    const { got, onMiss } = await postAll('aaaaaaaa-0000-4000-8000-000000000001', [{ id: 3, from: 'wren', to: 'kade', content: 'x' }]);
    expect(got[0].tags).toContainEqual(['h', 'aaaaaaaa-0000-4000-8000-000000000001']);
    expect(onMiss).not.toHaveBeenCalled();
  });

  test('NEGATIVE PROOF: a note the channel refuses is a miss, never a silent success', async () => {
    // the relay guards a different channel, as #team refused the old kind 1 t=team
    const { got, onMiss } = await postAll('ffffffff-0000-4000-8000-00000000000f', [{ id: 5, from: 'kade', to: 'silas', content: 'lost' }]);
    expect(onMiss).toHaveBeenCalledWith(expect.objectContaining({ id: got[0].id }), 'test relay: not a #team channel message', expect.any(Boolean));
  });
});
