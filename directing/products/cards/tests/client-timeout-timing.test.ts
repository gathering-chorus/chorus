// @test-type: integration:perf — a local stalling server on an ephemeral port; no live board.
// @needs: nightly — a 3s wall-clock limit measures the box (#4539: card runs leave it to the nightly)
// @domain: cards — placed by chorus-crawl's domain rules (#4538)
// @card: #3347
// @owner: wren
/**
 * #3347 / #4539 — cards CLI client timeout, timed. client-timeout.test.ts proves
 * the call fails with a timeout; this file proves it fails within 3s.
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

describe('#3347 client timeout, timed', () => {
  let stallServer: net.Server;
  let port: number;
  const sockets = new Set<net.Socket>();

  beforeAll(() => new Promise<void>((resolve) => {
    // Accepts TCP connections and never writes a byte — the worst case that
    // hung the real CLI (connection established, response never comes).
    stallServer = net.createServer((sock) => {
      sockets.add(sock); // track so teardown can destroy the held-open socket
      sock.on('close', () => sockets.delete(sock));
    });
    stallServer.listen(0, '127.0.0.1', () => {
      port = (stallServer.address() as net.AddressInfo).port;
      resolve();
    });
  }));

  afterAll(() => new Promise<void>((resolve) => {
    for (const s of sockets) s.destroy(); // close() waits on live conns otherwise
    stallServer.close(() => resolve());
  }));

  test('api call against a stalling server rejects within 3s', async () => {
    process.env.CARDS_API_TIMEOUT_MS = '500';
    const client = new BoardClient(`http://127.0.0.1:${port}`, 'test-token', { projectId: 1, buckets: {} } as never);

    const started = Date.now();
    await expect(
      client.fetchBuckets()
    ).rejects.toThrow(/timeout/i);
    expect(Date.now() - started).toBeLessThan(3000);
    delete process.env.CARDS_API_TIMEOUT_MS;
  });

});
