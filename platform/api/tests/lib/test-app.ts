/**
 * In-process test harness — #2173 AC4.
 *
 * Imports the chorus-api Express app without binding to :3340 (the
 * require.main guard in server.ts skips app.listen under jest, landed in
 * #2167). The harness starts a throwaway listener on port 0, returns a
 * fetch-able base URL, and exposes a close() for teardown. Tests use this
 * instead of fetch('http://localhost:3340/...') so:
 *
 *   1. Jest instruments server.ts + handlers — coverage becomes real.
 *      Today jest shows 0% on server.ts because the live :3340 is a
 *      separate Node process jest can't measure.
 *   2. Parallel workers don't contend for a shared port — maxWorkers can
 *      go above 1 once tests are converted; suite runtime drops
 *      from minutes to seconds.
 *   3. Tests don't require the LaunchAgent service to be running — hermetic.
 *
 * Usage:
 *
 *   import { startTestApp, TestApp } from './lib/test-app';
 *
 *   let harness: TestApp;
 *   beforeAll(async () => { harness = await startTestApp(); });
 *   afterAll(async () => { await harness.close(); });
 *
 *   test('GET /health', async () => {
 *     const res = await fetch(`${harness.baseUrl}/health`);
 *     expect(res.status).toBe(200);
 *   });
 */

import type { Server } from 'http';
import type { AddressInfo } from 'net';

// #3379 — the on-demand index/embed/reindex routes spawn detached workers;
// tests must never fire real passes against live ~/.chorus state.
process.env.CHORUS_EMBED_WORKER_SCRIPT = '/usr/bin/true';
process.env.CHORUS_REINDEX_WORKER_SCRIPT = '/usr/bin/true';

export interface TestApp {
  baseUrl: string;
  port: number;
  close: () => Promise<void>;
}

export async function startTestApp(): Promise<TestApp> {
  // Import AFTER env is set so config-reading modules see test values.
  // The server module default-exports the Express app instance.
  const { default: app } = await import('../../src/server');

  const server: Server = await new Promise((resolve, reject) => {
    const srv = app.listen(0, '127.0.0.1', () => resolve(srv));
    srv.once('error', reject);
  });

  const address = server.address() as AddressInfo;
  const port = address.port;
  const baseUrl = `http://127.0.0.1:${port}`;

  return {
    baseUrl,
    port,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((err) => (err ? reject(err) : resolve()));
      }),
  };
}
