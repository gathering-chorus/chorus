/**
 * #3644 — same-origin /owl proxy: the Athena page family reaches owl-api
 * through chorus-api instead of hardcoding hostname:3360 (which broke every
 * off-LAN origin, live specimen: the share tunnel).
 * Hermetic: OWL_UPSTREAM points at a stub server in this test — no live :3360.
 */
// @test-type: api
import { createServer, Server } from 'http';
import type { AddressInfo } from 'net';
import app from '../src/server';

let stub: Server;

beforeAll(async () => {
  stub = createServer((req, res) => {
    if (req.method !== 'GET') {
      res.writeHead(500);
      return res.end('stub: write verb reached upstream — proxy must never forward these');
    }
    if (req.url === '/products') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      return res.end(JSON.stringify({ data: [{ name: 'werk' }] }));
    }
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'not found' }));
  });
  await new Promise<void>((r) => stub.listen(0, '127.0.0.1', () => r()));
  // the proxy reads OWL_UPSTREAM per-request, so setting it post-import is honored
  process.env.OWL_UPSTREAM = `http://127.0.0.1:${(stub.address() as AddressInfo).port}`;
});

afterAll(async () => {
  delete process.env.OWL_UPSTREAM;
  await new Promise<void>((r) => stub.close(() => r()));
});

async function hit(srv: Server, path: string, method = 'GET') {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  const port = (srv.address() as AddressInfo).port;
  const res = await fetch(`http://127.0.0.1:${port}${path}`, { method });
  return { status: res.status, body: await res.text() };
}

const close = (srv: Server) => new Promise<void>((r) => srv.close(() => r()));

describe('#3644 — /owl same-origin proxy', () => {
  test('GET /owl/products proxies the model API', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/owl/products');
      expect(r.status).toBe(200);
      expect(JSON.parse(r.body).data[0].name).toBe('werk');
    } finally {
      await close(srv);
    }
  });

  test('upstream 404 passes through honestly', async () => {
    const srv = app.listen(0);
    try {
      const r = await hit(srv, '/owl/no-such-collection');
      expect(r.status).toBe(404);
    } finally {
      await close(srv);
    }
  });

  test('write verbs are 405 at the proxy — never reach owl-api', async () => {
    const srv = app.listen(0);
    try {
      for (const method of ['POST', 'PUT', 'DELETE', 'PATCH']) {
        const r = await hit(srv, '/owl/products', method);
        expect(r.status).toBe(405);
      }
    } finally {
      await close(srv);
    }
  });
});
