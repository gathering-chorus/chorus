// @test-type: unit — a signed fake Jeff note and a pulse address nothing listens on; no live relay, no pulse.
// @card: #4649
// @owner: wren
// @domain: messages
/**
 * #4649 AC5 — end to end on the server: Jeff's message names a role, the hand-off to
 * pulse fails even after its retry, and the page is told "not delivered to <role>",
 * live and in the list every new page receives.
 */
import http from 'http';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { makeInboundHandler } from '../src/server';
import { MessageRouter } from '../src/router';
import { UndeliveredLog } from '../src/undelivered';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const jeffSk = new Uint8Array(32).fill(71);
const jeffPk = bytesToHex(schnorr.getPublicKey(jeffSk));
const keys = new Map([[jeffPk, 'jeff']]);

function jeffNote(content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['room', 'team']];
  const id = buildEventId(serializeEvent(jeffPk, created_at, 1, tags, content));
  return { id, pubkey: jeffPk, created_at, kind: 1, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), jeffSk)) };
}

async function deadPulseUrl(): Promise<string> {
  const s = http.createServer();
  await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
  const port = (s.address() as { port: number }).port;
  await new Promise<void>((r) => s.close(() => r()));
  return `http://127.0.0.1:${port}`;
}

const settle = () => new Promise((r) => setTimeout(r, 400));

test('a hand-off that fails after its retry tells the page "not delivered", live and on reload', async () => {
  const events: Array<{ type: string; data: any }> = [];
  const undelivered = new UndeliveredLog();
  const handler = makeInboundHandler({
    router: new MessageRouter(), keys, pulseUrl: await deadPulseUrl(),
    broadcastSSE: (type, data) => events.push({ type, data }), isRelayConnected: () => true, undelivered,
  });
  const note = jeffNote('@silas what do i need to do to finish 4627');
  handler(note);
  await settle();
  const failure = events.find((e) => e.type === 'delivery');
  expect(failure?.data).toMatchObject({ ok: false, to: 'silas', roles: ['silas'] });
  expect(Object.values(undelivered.all())).toEqual([['silas']]);
});

test('NEGATIVE PROOF: a hand-off pulse accepts sends no "not delivered"', async () => {
  const server = http.createServer((_req, res) => { res.writeHead(202).end(); });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const events: Array<{ type: string; data: any }> = [];
  const undelivered = new UndeliveredLog();
  try {
    const handler = makeInboundHandler({
      router: new MessageRouter(), keys, pulseUrl: `http://127.0.0.1:${(server.address() as { port: number }).port}`,
      broadcastSSE: (type, data) => events.push({ type, data }), isRelayConnected: () => true, undelivered,
    });
    handler(jeffNote('@silas hello'));
    await settle();
    expect(events.filter((e) => e.type === 'delivery')).toHaveLength(0);
    expect(undelivered.all()).toEqual({});
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});
