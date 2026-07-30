// @test-type: integration — hits the LIVE self-hosted relay with the REAL bridge
// key. Gated: runs only when BUZZ_LIVE=1 and ~/.chorus/buzz/bridge.key exists
// (never in the default suite / CI). Proves #3696's AC "attribution provable on
// the relay" through the ACTUAL modules — buzz-signer + buzz-bridge + buzz-relay —
// not a reimplementation. A bad sig, a missing channel, or a non-admitted key all
// fail HERE (the relay is the judge), which is the honest place to prove crypto.
import fs from 'fs';
import os from 'os';
import path from 'path';
import WebSocket from 'ws';
import { buildKind9 } from '../src/buzz-bridge';
import { nobleSigner } from '../src/buzz-signer';
import { publishEvent, type RelaySocket } from '../src/buzz-relay';

const KEY_FILE = path.join(os.homedir(), '.chorus', 'buzz', 'bridge.key');
const RELAY = process.env.BUZZ_RELAY_URL || 'ws://192.168.86.242:3000';
const CHANNEL = process.env.BUZZ_TEAM_CHANNEL || '160e1f5d-6816-4c68-94b3-b5f1ade75565';
const LIVE = process.env.BUZZ_LIVE === '1' && fs.existsSync(KEY_FILE);

(LIVE ? describe : describe.skip)('#3696 LIVE — Clearing message → signed kind:9 → relay accepts', () => {
  it('publishes a real signed event into #team and the relay returns OK accepted=true', async () => {
    const keyHex = fs.readFileSync(KEY_FILE, 'utf8').trim();
    const signer = nobleSigner(keyHex);
    const msg = {
      from: 'wren',
      text: '#3696 live proof — the Clearing speaks Buzz (signed kind:9 mirror, Leg A)',
      ts: new Date().toISOString(),
      type: 'role-response',
      visible: true,
    };
    const ev = buildKind9(msg, CHANNEL, signer);
    // author preserved in content, bridge signs as itself — never a forged pubkey
    expect(ev.content).toMatch(/^\[wren\]/);
    expect(ev.pubkey).toBe(signer.pubkey);

    const result = await publishEvent(
      { relayUrl: RELAY, signer, connect: (url) => new WebSocket(url) as unknown as RelaySocket, nowSec: () => Math.floor(Date.now() / 1000) },
      ev,
    );
    console.log(`RELAY VERDICT: ok=${result.ok} pubkey=${signer.pubkey.slice(0, 12)}… msg="${result.message}"`);
    expect(result.ok).toBe(true);
  }, 15000);
});
