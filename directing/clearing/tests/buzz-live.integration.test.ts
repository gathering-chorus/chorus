// @test-type: integration — hits the LIVE self-hosted relay with the REAL bridge
// key. Gated: runs only when BUZZ_LIVE=1 and ~/.chorus/buzz/bridge.key exists
// (never in the default suite / CI). Proves "attribution provable on the relay"
// through the ACTUAL modules — buzz-signer + buzz-bridge + buzz-relay —
// not a reimplementation. A bad sig, a missing channel, or a non-admitted key all
// fail HERE (the relay is the judge), which is the honest place to prove crypto.
import fs from 'fs';
import os from 'os';
import path from 'path';
import WebSocket from 'ws';
import { buildNote } from '../src/buzz-bridge';
import { nobleSigner } from '../src/buzz-signer';
import { publishEvent, type RelaySocket } from '../src/buzz-relay';

const KEY_FILE = path.join(os.homedir(), '.chorus', 'buzz', 'bridge.key');
const RELAY = process.env.BUZZ_RELAY_URL || 'ws://192.168.86.242:3000';
const TOPIC = process.env.BUZZ_ROOM_TOPIC || 'team';
const LIVE = process.env.BUZZ_LIVE === '1' && fs.existsSync(KEY_FILE);

// #3893: was kind:9 through the retired #3696 mirror. The mirror is deleted, so
// the live proof now exercises the ONE publisher that remains — the room's
// author-signed kind:1 note. Proving a path nothing runs is worse than no proof.
(LIVE ? describe : describe.skip)('LIVE — Clearing message → author-signed kind:1 note → relay accepts', () => {
  it('publishes a real signed event into #team and the relay returns OK accepted=true', async () => {
    const keyHex = fs.readFileSync(KEY_FILE, 'utf8').trim();
    const signer = nobleSigner(keyHex);
    const msg = {
      from: 'wren',
      text: '#3893 live proof — the Clearing speaks Buzz (author-signed kind:1 note)',
      ts: new Date().toISOString(),
      type: 'role-response',
      visible: true,
    };
    const ev = buildNote(msg, TOPIC, signer);
    // The author IS the signature now — no name in the body to read instead.
    expect(ev.content).toBe(msg.text);
    expect(ev.tags).toContainEqual(['t', TOPIC]);
    expect(ev.pubkey).toBe(signer.pubkey);

    const result = await publishEvent(
      { relayUrl: RELAY, signer, connect: (url) => new WebSocket(url) as unknown as RelaySocket, nowSec: () => Math.floor(Date.now() / 1000) },
      ev,
    );
    console.log(`RELAY VERDICT: ok=${result.ok} pubkey=${signer.pubkey.slice(0, 12)}… msg="${result.message}"`);
    expect(result.ok).toBe(true);
  }, 15000);
});
