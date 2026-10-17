// @test-type: unit
// @domain: messages
// @card: #4581
// @owner: abby-normal
// #4581 — numeric path (/1 or /clearing-v2/1) redirects to the board with card drawer open
import http from 'http';
import { createApp } from '../src/server';

test('numeric paths redirect to /board?card=N under both LAN and /clearing-v2 prefix', async () => {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  try {
    const resTunnel = await fetch(`http://127.0.0.1:${port}/clearing-v2/1`, { redirect: 'manual' });
    expect(resTunnel.status).toBe(302);
    expect(resTunnel.headers.get('location')).toBe('/clearing-v2/board?card=1');

    const resLan = await fetch(`http://127.0.0.1:${port}/4580`, { redirect: 'manual' });
    expect(resLan.status).toBe(302);
    expect(resLan.headers.get('location')).toBe('/board?card=4580');
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});

test('negative proof: non-numeric path does not redirect and 404s', async () => {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  try {
    const res = await fetch(`http://127.0.0.1:${port}/clearing-v2/not-a-card`, { redirect: 'manual' });
    expect(res.status).toBe(404);
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});

test('negative proof: alphanumeric path with leading digits (/1abc) does not redirect and 404s', async () => {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  try {
    const res = await fetch(`http://127.0.0.1:${port}/1abc`, { redirect: 'manual' });
    expect(res.status).toBe(404);
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});

test('negative proof: named routes like /board are not intercepted and still return 200', async () => {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as { port: number }).port;
  try {
    const res = await fetch(`http://127.0.0.1:${port}/board`, { redirect: 'manual' });
    expect(res.status).toBe(200);
  } finally {
    await new Promise<void>((r) => server.close(() => r()));
  }
});
