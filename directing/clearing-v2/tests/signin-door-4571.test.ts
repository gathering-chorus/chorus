// @test-type: unit
// @domain: messages
// @card: #4571
// @owner: wren
// #4571 — Silas 2026-10-16 19:59: no second sign-in. On the LAN address v2 has no
// session, so join and send are refused with a 401 that names why and where to sign in
// (the common door's v2 host); a signed-in browser is not sent away.
import http from 'http';
import { createApp } from '../src/server';

const DOOR = 'https://clearing-v2.lightlifeurbangardens.com/';

async function withApp(signInUrl: string | undefined, fn: (port: number) => Promise<void>) {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null, signInUrl });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  try { await fn((server.address() as { port: number }).port); } finally { await new Promise<void>((r) => server.close(() => r())); }
}

const post = (port: number, p: string, body: unknown) =>
  fetch(`http://127.0.0.1:${port}${p}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

test('send with no session: 401 naming why and the door to sign in at', async () => {
  await withApp(DOOR, async (port) => {
    const res = await post(port, '/api/send', { event: { id: 'x', sig: 'y' } });
    expect(res.status).toBe(401);
    const j = (await res.json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.error).toBe('not-signed-in');
    expect(j.signInUrl).toBe(DOOR);
    expect(j.detail).toContain(DOOR);
  });
});

test('join (bind) with no session: same 401 and door', async () => {
  await withApp(DOOR, async (port) => {
    const res = await post(port, '/api/room/bind', { pubkey: 'a'.repeat(64) });
    expect(res.status).toBe(401);
    const j = (await res.json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.signInUrl).toBe(DOOR);
  });
});

test('negative proof: with no door configured the refusal still names why, and invents no URL', async () => {
  await withApp(undefined, async (port) => {
    const j = (await (await post(port, "/api/send", { event: { id: "x", sig: "y" } })).json()) as { error?: string; signInUrl?: string; detail?: string };
    expect(j.error).toBe('not-signed-in');
    expect(j.signInUrl).toBeUndefined();
    expect(j.detail).toMatch(/sign in/i);
  });
});
