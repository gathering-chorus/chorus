// @test-type: unit — the app on a local port; no relay, no signer.
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 reopen 2 — the room's client key, which the signing service must allow, is read
// from /health (not a log line), so Silas can put it in the signer's plist.
import http from 'http';
import { createApp } from '../src/server';

async function health(signerClientKey?: string): Promise<Record<string, unknown>> {
  const { app, stopKeys } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null, knownKeys: new Map(), signerClientKey });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  try {
    return await (await fetch(`http://127.0.0.1:${(server.address() as { port: number }).port}/health`)).json() as Record<string, unknown>;
  } finally { stopKeys?.(); await new Promise<void>((r) => server.close(() => r())); }
}

describe('#4652 /health names the signer client key', () => {
  test('with the switch on, /health carries the room\'s client key', async () => {
    expect((await health('c'.repeat(64))).signerClientKey).toBe('c'.repeat(64));
  });

  test('NEGATIVE PROOF: with the switch off, /health says null, never a made-up key', async () => {
    expect((await health()).signerClientKey).toBeNull();
  });
});
