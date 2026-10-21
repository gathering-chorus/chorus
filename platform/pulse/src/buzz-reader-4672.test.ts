// @test-type: unit — a temp message store and a stub relay on a local socket; no live relay.
// @domain: messages
// @card: #4672
// @owner: wren
/**
 * #4672 — pulse's Buzz shadow sensor must see every message it stores, or it
 * cannot be trusted to gate delivery from Buzz (Jeff 10-21 09:02). Measured
 * 10-21 08:30–09:00: 12 rows stored, 10 verdicts. The 2 without were Jeff's
 * Clearing posts (type jeff-input): the reader read #team only, the missed
 * sweep counted only nudges, and nothing tied his #clearing note to its row.
 * Clearing v2 now hands each role's copy off with traceId jeff-<signed id>-<to>
 * (Abby agreed 10:07), and the reader joins Jeff's notes on it.
 */
import http from 'http';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { WebSocketServer } from 'ws';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { MessageStore } from './store';
import { BuzzReader, jeffVerdicts, type ShadowCoverage, type ShadowVerdict } from './buzz-reader';
import { signNote, type BuzzNote } from './buzz-writer';

const TEAM = '160e1f5d-6816-4c68-94b3-b5f1ade75565';
const CLEARING = '03e2b251-240f-47fe-89b5-fe4e7ac648fb';
const sk = new Uint8Array(32).fill(72);
const signer = { pubkey: bytesToHex(schnorr.getPublicKey(sk)), sign: (id: string) => bytesToHex(schnorr.sign(hexToBytes(id), sk)) };

function tmpStore(): MessageStore {
  return new MessageStore(path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-reader-4672-')), 'messages.db'));
}
/** Jeff's post as Clearing v2 stores it in Buzz: his signed id in the `signed` tag. */
const clearingCopy = (signedId: string, content: string): BuzzNote =>
  signNote(signer, 1_800_000_000, 9, [['h', CLEARING], ['author', 'jeff'], ['signed', signedId], ['surface', 'clearing'], ['kind', 'jeff']], content);

const SIGNED = 'a'.repeat(64);

describe('#4672 a Jeff note in #clearing joins every role row it was handed off as', () => {
  test('one post to two roles: both rows match', () => {
    const store = tmpStore();
    const text = '@kade @silas look at this';
    const k = store.sendJeffInput('kade', text, `jeff-${SIGNED}-kade`);
    const s = store.sendJeffInput('silas', text, `jeff-${SIGNED}-silas`);
    expect(jeffVerdicts(clearingCopy(SIGNED, text), store)).toEqual([
      { msg: `pulse-${k}`, verdict: 'match', id: k },
      { msg: `pulse-${s}`, verdict: 'match', id: s },
    ]);
  });

  test("a post from the Buzz app (no signed tag) joins on the note's own id", () => {
    const store = tmpStore();
    const n = signNote(signer, 1_800_000_000, 9, [['h', CLEARING]], '@kade from my phone');
    const id = store.sendJeffInput('kade', '@kade from my phone', `jeff-${n.id}-kade`);
    expect(jeffVerdicts(n, store)).toEqual([{ msg: `pulse-${id}`, verdict: 'match', id }]);
  });

  test('NEGATIVE PROOF: different words in the row are a content-mismatch, never a match', () => {
    const store = tmpStore();
    const id = store.sendJeffInput('kade', '@kade the real words', `jeff-${SIGNED}-kade`);
    expect(jeffVerdicts(clearingCopy(SIGNED, '@kade other words'), store)).toEqual([{ msg: `pulse-${id}`, verdict: 'content-mismatch', id }]);
  });

  test('a note no row was handed off as gives no verdicts (a role reply, a room message)', () => {
    expect(jeffVerdicts(clearingCopy(SIGNED, 'nobody mentioned'), tmpStore())).toEqual([]);
  });

  test("NEGATIVE PROOF: another post's rows are never joined (the id is matched whole, not as a prefix)", () => {
    const store = tmpStore();
    store.sendJeffInput('kade', 'x', `jeff-${SIGNED}b-kade`);
    expect(jeffVerdicts(clearingCopy(SIGNED, 'x'), store)).toEqual([]);
  });
});

const relayWith = async (notes: BuzzNote[]) => {
  const reqs: Array<Record<string, unknown>> = [];
  const server = http.createServer();
  const wss = new WebSocketServer({ server });
  wss.on('connection', (ws) => ws.on('message', (raw) => {
    const f = JSON.parse((raw as Buffer).toString('utf8'));
    if (f[0] !== 'REQ') return;
    reqs.push(f[2]);
    const want = (f[2] as { '#h': string[] })['#h'];
    for (const n of notes) if (want.includes(n.tags.find((t) => t[0] === 'h')?.[1] ?? '')) ws.send(JSON.stringify(['EVENT', f[1], n]));
  }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `ws://127.0.0.1:${(server.address() as { port: number }).port}`, reqs,
    close: () => new Promise<void>((res) => { for (const c of wss.clients) c.terminate(); wss.close(() => server.close(() => res())); }) };
};
const until = (f: () => boolean) => new Promise<void>((res) => { const t = setInterval(() => { if (f()) { clearInterval(t); res(); } }, 20); });

describe('#4672 the reader reads #clearing as well as #team', () => {
  test("Jeff's #clearing post gets its verdict, and the subscription names both rooms", async () => {
    const store = tmpStore();
    const id = store.sendJeffInput('kade', '@kade hi', `jeff-${SIGNED}-kade`);
    const relay = await relayWith([clearingCopy(SIGNED, '@kade hi')]);
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: relay.url, signer, store, channel: TEAM, extraChannels: [CLEARING], onVerdict: (v) => verdicts.push(v), sweepEveryMs: 0 });
    try {
      await until(() => verdicts.length > 0);
      expect(relay.reqs[0]).toEqual(expect.objectContaining({ '#h': [TEAM, CLEARING] }));
      expect(verdicts).toEqual([{ msg: `pulse-${id}`, verdict: 'match' }]);
    } finally { r.close(); await relay.close(); }
  });

  test('NEGATIVE PROOF: without #clearing the same post is never judged', async () => {
    const store = tmpStore();
    store.sendJeffInput('kade', '@kade hi', `jeff-${SIGNED}-kade`);
    const relay = await relayWith([clearingCopy(SIGNED, '@kade hi')]);
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: relay.url, signer, store, channel: TEAM, onVerdict: (v) => verdicts.push(v), sweepEveryMs: 0 });
    try {
      await until(() => relay.reqs.length > 0);
      await new Promise((res) => setTimeout(res, 150));
      expect(verdicts).toEqual([]);
    } finally { r.close(); await relay.close(); }
  });
});

describe('#4672 the missed sweep covers Jeff input, and every sweep reports coverage', () => {
  test('NEGATIVE PROOF: a judged row whose words differ is not covered (matched / stored, Abby 10:17)', async () => {
    let now = Date.now() - 5_000;
    const store = tmpStore();
    store.sendJeffInput('kade', '@kade the real words', `jeff-${SIGNED}-kade`);
    const relay = await relayWith([clearingCopy(SIGNED, '@kade other words')]);
    const coverage: ShadowCoverage[] = [];
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: relay.url, signer, store, channel: TEAM, extraChannels: [CLEARING], onVerdict: (v) => verdicts.push(v),
      onCoverage: (c) => coverage.push(c), sweepEveryMs: 0, graceMs: 1_000, now: () => now - 10_000 });
    try {
      await until(() => verdicts.length > 0);
      now += 120_000;
      r.sweep();
      expect(coverage).toEqual([expect.objectContaining({ stored: 1, judged: 1, matched: 0, missed: 0, pct: 0 })]);
    } finally { r.close(); await relay.close(); }
  });


  test("NEGATIVE PROOF: Jeff's row with no note is reported missed (the 53915/53916 case)", async () => {
    let now = Date.now() - 5_000;
    const store = tmpStore();
    const relay = await relayWith([]);
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: relay.url, signer, store, channel: TEAM, onVerdict: (v) => verdicts.push(v), sweepEveryMs: 0, graceMs: 1_000, now: () => now });
    try {
      const id = store.sendJeffInput('kade', '@kade never copied');
      now += 60_000;
      r.sweep();
      expect(verdicts).toEqual([{ msg: `pulse-${id}`, verdict: 'missed' }]);
    } finally { r.close(); await relay.close(); }
  });

  test('coverage: stored, judged and missed per window, and 100% only when every row was judged', async () => {
    let now = Date.now() - 5_000;
    const store = tmpStore();
    const seen = store.sendJeffInput('kade', '@kade hi', `jeff-${SIGNED}-kade`);
    const relay = await relayWith([clearingCopy(SIGNED, '@kade hi')]);
    const coverage: ShadowCoverage[] = [];
    const verdicts: ShadowVerdict[] = [];
    const r = new BuzzReader({ relayUrl: relay.url, signer, store, channel: TEAM, extraChannels: [CLEARING], onVerdict: (v) => verdicts.push(v),
      onCoverage: (c) => coverage.push(c), sweepEveryMs: 0, graceMs: 1_000, now: () => now - 10_000 });
    try {
      await until(() => verdicts.length > 0);
      const lost = store.sendNudge('kade', 'silas', 'never copied');
      now += 120_000;
      r.sweep();
      expect(verdicts).toContainEqual({ msg: `pulse-${lost}`, verdict: 'missed' });
      expect(coverage).toEqual([{ stored: 2, judged: 1, matched: 1, missed: 1, pct: 50, from: expect.any(String), to: expect.any(String) }]);
      expect(seen).toBeGreaterThan(0);
    } finally { r.close(); await relay.close(); }
  });
});
