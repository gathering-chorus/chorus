// @test-type: unit — a fake relay on a local socket; no live relay, no pulse.
// @card: #4608
// @owner: silas
// @domain: messages
/**
 * #4608 — a message Jeff sends in Clearing v2 is stored in Buzz.
 *
 * Measured 2026-10-18: today's relay held 0 notes signed by Jeff's v2 key. The
 * relay refuses any note whose key is not the connection's AUTH identity
 * (["OK",id,false,"invalid: event pubkey does not match authenticated identity"]),
 * v2's connection is AUTHed as the bridge, and /api/send never read the OK. Pulse
 * still delivered, so the roles got the 30 messages and Buzz (and search) did not.
 *
 * The fake relay here enforces that same rule, so a v2 that publishes Jeff's own
 * note is refused exactly as the real one was.
 */
import http from 'http';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from '../src/server';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const jeffSk = new Uint8Array(32).fill(51);
const jeffPk = bytesToHex(schnorr.getPublicKey(jeffSk));
const bridgeSk = new Uint8Array(32).fill(52);
const bridge = { pubkey: bytesToHex(schnorr.getPublicKey(bridgeSk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), bridgeSk)) };

function jeffNote(content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['room', 'team']];
  const id = buildEventId(serializeEvent(jeffPk, created_at, 1, tags, content));
  return { id, pubkey: jeffPk, created_at, kind: 1, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), jeffSk)) };
}

/** A relay that stores only notes signed by the identity the connection is (the bridge), and echoes stored notes to subscribers. */
async function fakeRelay(acceptKey: string | null) {
  const stored: NostrEvent[] = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => {
    let sub = '';
    ws.on('message', (data) => {
      const f = JSON.parse(String(data));
      if (f[0] === 'REQ') sub = f[1];
      if (f[0] !== 'EVENT') return;
      const ev = f[1] as NostrEvent;
      if (acceptKey && ev.pubkey === acceptKey) {
        stored.push(ev);
        ws.send(JSON.stringify(['OK', ev.id, true, '']));
        if (sub) ws.send(JSON.stringify(['EVENT', sub, ev]));
      } else {
        ws.send(JSON.stringify(['OK', ev.id, false, 'invalid: event pubkey does not match authenticated identity']));
      }
    });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  const close = () => new Promise<void>((r) => wss.close(() => server.close(() => r())));
  return { url: `ws://127.0.0.1:${port}`, stored, close };
}

async function start(relayUrl: string) {
  const created = createApp({ relayUrl, pulseUrl: 'http://127.0.0.1:9', knownKeys: new Map([[jeffPk, 'jeff']]), relaySigner: bridge });
  created.relay.connect();
  for (let i = 0; i < 50 && !created.relay.isConnected(); i++) await new Promise((r) => setTimeout(r, 20));
  const server = http.createServer(created.app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  const send = async (ev: NostrEvent) => (await fetch(`http://127.0.0.1:${port}/api/send`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ event: ev }),
  })).json() as Promise<{ ok: boolean; stored?: boolean; reason?: string }>;
  const stop = async () => { created.relay.close(); await new Promise<void>((r) => server.close(() => r())); };
  return { ...created, send, stop };
}

describe('#4608 Jeff\'s v2 message is stored in Buzz', () => {
  test('one send stores exactly one note: bridge-signed, author=jeff, his signed id tagged, his words', async () => {
    const relay = await fakeRelay(bridge.pubkey);
    const v2 = await start(relay.url);
    try {
      const ev = jeffNote('is the counts page right?');
      const r = await v2.send(ev);
      expect(r).toEqual(expect.objectContaining({ ok: true, stored: true }));
      expect(relay.stored).toHaveLength(1);
      const note = relay.stored[0];
      expect(note.pubkey).toBe(bridge.pubkey);
      expect(note.content).toBe('is the counts page right?');
      expect(note.tags).toEqual(expect.arrayContaining([['h', '03e2b251-240f-47fe-89b5-fe4e7ac648fb'], ['t', 'team'], ['author', 'jeff'], ['signed', ev.id], ['surface', 'clearing'], ['kind', 'jeff']])); // #4628 where it was typed, and its kind
    } finally { await v2.stop(); await relay.close(); }
  });

  test('the stored copy echoing back does not show the message twice in the room', async () => {
    const relay = await fakeRelay(bridge.pubkey);
    const v2 = await start(relay.url);
    try {
      await v2.send(jeffNote('only once please'));
      await new Promise((r) => setTimeout(r, 100)); // the relay's echo arrives
      expect(v2.router.getRecent(100).filter((m) => m.text === 'only once please')).toHaveLength(1);
    } finally { await v2.stop(); await relay.close(); }
  });

  // NEGATIVE PROOF: a relay that refuses (the 10-18 state) is reported, never "stored".
  test('negative proof: a refused note is reported as not stored, with the relay\'s reason', async () => {
    const relay = await fakeRelay(null);
    const v2 = await start(relay.url);
    try {
      const r = await v2.send(jeffNote('this one is refused'));
      expect(r.ok).toBe(true); // the roles still get it
      expect(r.stored).toBe(false);
      expect(r.reason).toMatch(/does not match authenticated identity/);
      expect(relay.stored).toHaveLength(0);
    } finally { await v2.stop(); await relay.close(); }
  });
});

// #4608 — the page tells Jeff when Buzz did not store his message (the roles still got it).
describe('#4608 the page says when a message was not stored', () => {
  const ui = () => ({
    inputEl: { value: 'hello?' },
    statusEl: { textContent: '', style: { display: 'none' } },
    btnEl: { textContent: 'Send', className: '', classList: { add: jest.fn(), remove: jest.fn() } },
  });
  const reply = (body: unknown) => ({ ok: true, status: 200, json: async () => body, text: async () => JSON.stringify(body) });

  test('stored:false shows the relay\'s reason; the message still counts as sent', async () => {
    const { handleSendMessage } = require('../public/app.js');
    const u = ui();
    await handleSendMessage('hello?', { ...u, fetchFn: jest.fn(async () => reply({ ok: true, stored: false, reason: 'invalid: event pubkey does not match authenticated identity' })) });
    expect(u.statusEl.textContent).toMatch(/not stored in Buzz.*does not match/);
    expect(u.statusEl.style.display).not.toBe('none');
    expect(u.btnEl.textContent).toBe('Sent');
  });

  // NEGATIVE PROOF: a stored message shows no warning.
  test('negative proof: stored:true shows nothing', async () => {
    const { handleSendMessage } = require('../public/app.js');
    const u = ui();
    await handleSendMessage('hello?', { ...u, fetchFn: jest.fn(async () => reply({ ok: true, stored: true })) });
    expect(u.statusEl.textContent).toBe('');
  });
});
