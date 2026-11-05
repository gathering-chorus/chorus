// @test-type: integration — the real app on a local port, a stand-in pulse; no relay, no real key.
// @domain: messages
// @card: #4889
// @owner: silas
/**
 * #4889 (Jeff 11-05 11:14: "both abby and bq have gotten 0 messages in agy except from me").
 * His 11-04 18:27 post in the @ borg-queen room was signed room 'team' by public/app.js,
 * so src/router.ts sent it to the moderator: messages.db row 67493, to: wren. Now the
 * composer signs the room the page is (roomFromPath), and the post reaches that role alone.
 */
import http from 'http';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { createApp } from '../src/server';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';
const client = require('../public/app.js');

const sk = new Uint8Array(32).fill(89);
const pk = bytesToHex(schnorr.getPublicKey(sk));

function note(room: string, content: string): NostrEvent {
  const created_at = Math.floor(Date.now() / 1000);
  const tags = [['room', room]];
  const id = buildEventId(serializeEvent(pk, created_at, 1, tags, content));
  return { id, pubkey: pk, created_at, kind: 1, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), sk)) };
}

/** a stand-in pulse that records who each jeff-input was addressed to */
async function fakePulse() {
  const to: string[] = [];
  const server = http.createServer((req, res) => {
    let b = '';
    req.on('data', (c) => { b += c; });
    req.on('end', () => { if (req.url === '/api/jeff-input') to.push(JSON.parse(b).to); res.end('{"ok":true}'); });
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, to, close: () => new Promise<void>((r) => server.close(() => r())) };
}

async function postedTo(room: string): Promise<string[]> {
  const pulse = await fakePulse();
  const created = createApp({ relayUrl: '', pulseUrl: pulse.url, knownKeys: new Map([[pk, 'jeff']]), relaySigner: null });
  created.router.setModerator('wren');
  const server = http.createServer(created.app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  await fetch(`http://127.0.0.1:${port}/api/send`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ event: note(room, 'nothing') }) });
  for (let i = 0; i < 50 && pulse.to.length === 0; i++) await new Promise((r) => setTimeout(r, 20));
  await new Promise((r) => setTimeout(r, 50));
  await new Promise<void>((r) => server.close(() => r()));
  created.stopKeys();
  await pulse.close();
  return pulse.to;
}

describe('#4889 Jeff\'s post reaches the role whose room he is in, and no other', () => {
  it('the composer signs the room the page is', async () => {
    expect(client.roomFromPath('/rooms/borg-queen')).toBe('borg-queen');
    expect(client.roomFromPath('/clearing/rooms/abby-normal')).toBe('abby-normal');
    expect(client.roomFromPath('/')).toBe('team');
    // a malformed escape never throws while Jeff is sending
    expect(client.roomFromPath('/rooms/borg%E0%A4%A')).toBe('borg%E0%A4%A');
    let sent: { event: NostrEvent } | undefined;
    const fetchFn = jest.fn().mockImplementation(async (_u: string, init: { body: string }) => {
      sent = JSON.parse(init.body);
      return { ok: true, status: 200, json: async () => ({ ok: true, stored: true }) };
    });
    await client.handleSendMessage('nothing', { inputEl: { value: 'nothing' }, statusEl: { textContent: '', style: {} }, fetchFn, pathname: '/rooms/borg-queen' });
    expect(sent?.event.tags).toEqual([['room', 'borg-queen']]);
  });

  it('a post in the borg-queen room goes to borg-queen alone', async () => {
    expect(await postedTo('borg-queen')).toEqual(['borg-queen']);
  });

  it('an abby-normal room post goes to abby-normal alone', async () => {
    expect(await postedTo('abby-normal')).toEqual(['abby-normal']);
  });

  it('NEGATIVE PROOF: the same post signed room team (the old composer) lands with to: wren', async () => {
    expect(await postedTo('team')).toEqual(['wren']);
  });
});
