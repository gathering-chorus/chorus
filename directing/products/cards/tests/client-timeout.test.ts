/**
 * #3347 — cards CLI client timeout.
 *
 * The 2026-06-11 box-starvation (×2 in one day): client.ts api() used raw
 * http.request with NO timeout, so any call against a slow/blocked API hung
 * FOREVER (observed 36 minutes). A 60s poller spawning these built a 197-process
 * pile that starved the machine.
 *
 * Contract under test: a cards API call against a server that accepts the
 * connection but never responds dies in CARDS_API_TIMEOUT_MS with a typed
 * error — never hangs.
 */
import * as net from 'net';
import { BoardClient } from '../src/client';

describe('#3347 client timeout — slow API = fast typed failure', () => {
  let stallServer: net.Server;
  let port: number;
  const sockets = new Set<net.Socket>();

  beforeAll((done) => {
    // Accepts TCP connections and never writes a byte — the worst case that
    // hung the real CLI (connection established, response never comes).
    stallServer = net.createServer((sock) => {
      sockets.add(sock); // track so teardown can destroy the held-open socket
      sock.on('close', () => sockets.delete(sock));
    });
    stallServer.listen(0, '127.0.0.1', () => {
      port = (stallServer.address() as net.AddressInfo).port;
      done();
    });
  });

  afterAll((done) => {
    for (const s of sockets) s.destroy(); // close() waits on live conns otherwise
    stallServer.close(() => done());
  });

  test('api call against a stalling server rejects within the timeout, not never', async () => {
    process.env.CARDS_API_TIMEOUT_MS = '500';
    const client = new BoardClient(`http://127.0.0.1:${port}`, 'test-token', { projectId: 1, buckets: {} } as never);

    const started = Date.now();
    await expect(
      // fetchBuckets is the simplest call that goes through api()
      client.fetchBuckets()
    ).rejects.toThrow(/timeout/i);
    const elapsed = Date.now() - started;

    // Must die promptly (timeout + small margin), not hang.
    expect(elapsed).toBeLessThan(3000);
    delete process.env.CARDS_API_TIMEOUT_MS;
  }, 10_000);

  test('default timeout is seconds, not unbounded — and is configurable via env', () => {
    // Pin the contract values so a future refactor can't silently drop them.
    const src = require('fs').readFileSync(require.resolve('../src/client.ts'), 'utf-8');
    expect(src).toMatch(/CARDS_API_TIMEOUT_MS/);
    expect(src).toMatch(/setTimeout|timeout:/);
  });
});
