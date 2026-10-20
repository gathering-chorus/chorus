// @test-type: unit — signed fake Jeff app posts and a stub pulse on a local port; no live relay.
// @card: #4652
// @owner: wren
// @domain: messages
/**
 * #4652 AC2 — a message Jeff types in the Buzz app reaches the role it names, once.
 * Measured 10-20 06:54: five of Jeff's app posts (10-09 "wren can u see this?",
 * 10-19 "hello") sat in #team with nothing reading them. The relay replays history
 * on every connect, so the room must hand off each post once, across restarts,
 * and never re-send what was typed before the room started handing off.
 */
import fs from 'fs';
import http from 'http';
import os from 'os';
import path from 'path';
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils';
import { makeInboundHandler, resolveHandoffFile } from '../src/server';
import { MessageRouter } from '../src/router';
import { HandoffLog } from '../src/handoff-log';
import { serializeEvent, buildEventId } from '../src/identity';
import { NostrEvent } from '../src/types';

const appSk = new Uint8Array(32).fill(83);
const appPk = bytesToHex(schnorr.getPublicKey(appSk));
const CH = '160e1f5d-6816-4c68-94b3-b5f1ade75565';

function appPost(content: string, created_at = Math.floor(Date.now() / 1000)): NostrEvent {
  const tags = [['h', CH]];
  const id = buildEventId(serializeEvent(appPk, created_at, 9, tags, content));
  return { id, pubkey: appPk, created_at, kind: 9, tags, content, sig: bytesToHex(schnorr.sign(hexToBytes(id), appSk)) };
}

async function stubPulse(): Promise<{ url: string; got: string[]; close: () => Promise<void> }> {
  const got: string[] = [];
  const server = http.createServer((req, res) => { let b = ''; req.on('data', (c) => { b += c; }); req.on('end', () => { got.push(b); res.writeHead(202).end(); }); });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  return { url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, got, close: () => new Promise<void>((r) => server.close(() => r())) };
}

const settle = () => new Promise((r) => setTimeout(r, 300));

describe('#4652 the hand-off log', () => {
  test('a post is handed off once, even when the relay replays it', () => {
    const log = new HandoffLog(undefined, 100);
    expect(log.claim('a', 200)).toBe(true);
    expect(log.claim('a', 200)).toBe(false);
  });

  test('NEGATIVE PROOF: a post typed before the room started handing off is never re-sent', () => {
    const log = new HandoffLog(undefined, 100);
    expect(log.claim('old', 99)).toBe(false);
    expect(log.claim('new', 100)).toBe(true);
  });

  test('the start time and the handled posts survive a restart', () => {
    const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'handoff-4652-')), 'handoff.json');
    expect(new HandoffLog(file, 100).claim('a', 150)).toBe(true);
    const next = new HandoffLog(file, 999);
    expect(next.claim('a', 150)).toBe(false);
    expect(next.claim('b', 150)).toBe(true);
    expect(fs.existsSync(`${file}.tmp`)).toBe(false);
  });
});

describe('#4652 a Buzz app post from Jeff reaches the role it names', () => {
  test('@silas in the app is handed to silas once; the replay after a reconnect is not', async () => {
    const pulse = await stubPulse();
    try {
      const handoffs = new HandoffLog(undefined, 0);
      const make = () => makeInboundHandler({
        router: new MessageRouter(), keys: new Map([[appPk, 'jeff']]), pulseUrl: pulse.url,
        broadcastSSE: () => undefined, isRelayConnected: () => true, handoffs,
      });
      const post = appPost('@silas hello from the app');
      make()(post);
      await settle();
      make()(post); // a fresh router, as after a restart: the relay replays the post
      await settle();
      expect(pulse.got).toHaveLength(1);
      expect(JSON.parse(pulse.got[0]).to).toBe('silas');
    } finally {
      await pulse.close();
    }
  });

  test('NEGATIVE PROOF: an app key the room does not know is shown but never handed off', async () => {
    const pulse = await stubPulse();
    try {
      const handler = makeInboundHandler({
        router: new MessageRouter(), keys: new Map(), pulseUrl: pulse.url,
        broadcastSSE: () => undefined, isRelayConnected: () => true, handoffs: new HandoffLog(undefined, 0),
      });
      handler(appPost('@silas hello'));
      await settle();
      expect(pulse.got).toEqual([]);
    } finally {
      await pulse.close();
    }
  });
});

describe('#4652 prod v2 keeps the hand-off log on disk; tests never touch ~/.chorus', () => {
  test('prod start (no jest, no override) uses ~/.chorus/clearing-v2-handoffs.json', () => {
    expect(resolveHandoffFile(undefined, {})).toMatch(/\.chorus\/clearing-v2-handoffs\.json$/);
  });
  test('NEGATIVE PROOF: under jest with no path given, there is no log file', () => {
    expect(resolveHandoffFile(undefined, { JEST_WORKER_ID: '1' })).toBeUndefined();
    expect(resolveHandoffFile('/tmp/h.json', { JEST_WORKER_ID: '1' })).toBe('/tmp/h.json');
  });
});
