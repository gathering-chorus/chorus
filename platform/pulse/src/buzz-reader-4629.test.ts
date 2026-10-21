// @test-type: unit — a temp message store and a stub relay on a local socket; no live relay.
// @domain: messages
// @card: #4629
// @owner: wren
/**
 * #4629 S1 — pulse reads the room in shadow. For each #team note it says
 * whether delivering from Buzz would deliver the same message pulse delivered
 * from its own store, joined exactly by the note's `msg` tag (pulse-<id>), never
 * by text. S3 (deliver from Buzz) is gated on a full day of 100% match (Silas).
 */
import http from 'http';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { MessageStore } from './store';
import { BuzzReader, shadowVerdict, type ShadowVerdict } from './buzz-reader';
import { identitySignerFromHome, signNote, type BuzzNote } from './buzz-writer';

const TEAM = '160e1f5d-6816-4c68-94b3-b5f1ade75565';
const sk = new Uint8Array(32).fill(71);
const reader = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

function tmpStore(): MessageStore {
  return new MessageStore(path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-reader-4629-')), 'messages.db'));
}
const note = (tags: string[][], content = 'hi'): BuzzNote => signNote(reader, 1_800_000_000, 9, [['h', TEAM], ...tags], content);

describe('#4629 S1 shadowVerdict joins a room note to the store by its msg tag', () => {
  test('a nudge note whose msg row exists, to the same role, is a match', () => {
    const store = tmpStore();
    const id = store.sendNudge('kade', 'silas', 'hi');
    expect(shadowVerdict(note([['author', 'kade'], ['kind', 'nudge'], ['to', 'silas'], ['msg', `pulse-${id}`]]), store))
      .toEqual<ShadowVerdict>({ msg: `pulse-${id}`, verdict: 'match' });
  });

  test('NEGATIVE PROOF: the note names a different recipient than the row', () => {
    const store = tmpStore();
    const id = store.sendNudge('kade', 'silas', 'hi');
    expect(shadowVerdict(note([['author', 'kade'], ['kind', 'nudge'], ['to', 'wren'], ['msg', `pulse-${id}`]]), store).verdict).toBe('to-mismatch');
  });

  test('NEGATIVE PROOF: a msg tag with no row, and a note with no msg tag', () => {
    const store = tmpStore();
    expect(shadowVerdict(note([['kind', 'nudge'], ['to', 'silas'], ['msg', 'pulse-999999']]), store).verdict).toBe('no-row');
    expect(shadowVerdict(note([['kind', 'nudge'], ['to', 'silas']]), store).verdict).toBe('no-msg-tag');
  });

  test('a note with no recipient tag cannot be delivered from Buzz', () => {
    const store = tmpStore();
    const id = store.sendNudge('kade', 'silas', 'hi');
    expect(shadowVerdict(note([['kind', 'nudge'], ['msg', `pulse-${id}`]]), store).verdict).toBe('no-to-tag');
  });

  test('notes pulse does not deliver (replies, land notices, terminal copies) are not judged', () => {
    const store = tmpStore();
    expect(shadowVerdict(note([['kind', 'reply']]), store).verdict).toBe('not-a-delivery');
    expect(shadowVerdict(note([['kind', 'land'], ['msg', 'pulse-land-1']]), store).verdict).toBe('not-a-delivery');
    expect(shadowVerdict(note([['kind', 'jeff'], ['surface', 'terminal'], ['msg', 'pulse-term-1']]), store).verdict).toBe('not-a-delivery');
  });
});

describe('#4629 S1 BuzzReader subscribes to #team and judges each new note, delivering nothing', () => {
  test('signs in with its own key, subscribes to kind 9 #team, and reports a verdict per note', async () => {
    const store = tmpStore();
    const id = store.sendNudge('kade', 'silas', 'hi');
    const sent = note([['author', 'kade'], ['kind', 'nudge'], ['to', 'silas'], ['msg', `pulse-${id}`]]);
    const reqs: Array<Record<string, unknown>> = [];
    const authed: string[] = [];
    const server = http.createServer();
    const wss = new WebSocketServer({ server });
    wss.on('connection', (ws) => {
      ws.send(JSON.stringify(['AUTH', 'chal-1']));
      ws.on('message', (raw) => {
        const f = JSON.parse((raw as Buffer).toString('utf8'));
        if (f[0] === 'AUTH') authed.push(f[1].pubkey);
        if (f[0] === 'REQ') { reqs.push(f[2]); ws.send(JSON.stringify(['EVENT', f[1], sent])); }
      });
    });
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const url = `ws://127.0.0.1:${(server.address() as { port: number }).port}`;
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: url, signer: reader, store, channel: TEAM, onVerdict: (v) => verdicts.push(v) });
    try {
      await new Promise<void>((res) => { const t = setInterval(() => { if (verdicts.length) { clearInterval(t); res(); } }, 20); });
      expect(authed).toEqual([reader.pubkey]);
      expect(reqs[0]).toEqual(expect.objectContaining({ kinds: [9], '#h': [TEAM] }));
      expect(verdicts).toEqual([{ msg: `pulse-${id}`, verdict: 'match' }]);
      expect(store.getDeliveryRecord(id).delivery_status).toBe('pending');
    } finally {
      r.close();
      await new Promise<void>((res) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => res())); });
    }
  });
});

describe('#4629 S1 the reader signs in as principal-pulse, never as the bridge', () => {
  const home = () => fs.mkdtempSync(path.join(os.tmpdir(), 'pulse-id-4629-'));
  const put = (h: string, who: string, body: unknown) => {
    /* eslint-disable security/detect-non-literal-fs-filename -- a temp home this test made */
    fs.mkdirSync(path.join(h, '.chorus', 'identity', who), { recursive: true });
    fs.writeFileSync(path.join(h, '.chorus', 'identity', who, 'nostr.json'), JSON.stringify(body), { mode: 0o600 });
    /* eslint-enable security/detect-non-literal-fs-filename */
  };

  test('the pulse identity file gives a signer for its own pubkey', () => {
    const h = home();
    put(h, 'pulse', { seckey: bytesToHex(sk), pubkey: reader.pubkey });
    expect(identitySignerFromHome(h, 'pulse').signer?.pubkey).toBe(reader.pubkey);
  });

  test('NEGATIVE PROOF: no pulse file means no reader, even when the bridge key is there', () => {
    const h = home();
    put(h, 'bridge', { seckey: bytesToHex(sk), pubkey: reader.pubkey });
    expect(identitySignerFromHome(h, 'pulse')).toEqual({ reason: expect.stringContaining('identity/pulse/nostr.json') });
  });

  test('NEGATIVE PROOF: a file whose pubkey does not match its key is refused', () => {
    const h = home();
    put(h, 'pulse', { seckey: bytesToHex(sk), pubkey: 'f'.repeat(64) });
    expect(identitySignerFromHome(h, 'pulse').signer).toBeUndefined();
  });
});
