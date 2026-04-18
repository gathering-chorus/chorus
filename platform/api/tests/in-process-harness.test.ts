/**
 * In-process harness smoke — #2173 AC4.
 *
 * Minimum viable test: import `app` via the harness and hit /health
 * (liveness, zero external dependencies at server.ts:3965). If this
 * passes with jest instrumenting server.ts, the three architectural
 * prerequisites for converting the rest of the HTTP suite are proven:
 *
 *   1. `import app` works under ts-jest — the default export is reachable.
 *   2. `app.listen(0)` produces a bindable port reachable via fetch.
 *   3. Jest coverage now counts `/health`'s two lines in server.ts.
 *
 * This test does NOT gate on RUN_INTEGRATION — it's hermetic, always runs.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('in-process harness (#2173 AC4)', () => {
  let harness: TestApp;

  beforeAll(async () => {
    harness = await startTestApp();
  });

  afterAll(async () => {
    if (harness) await harness.close();
  });

  test('starts on an ephemeral port', () => {
    expect(harness.port).toBeGreaterThan(0);
    expect(harness.baseUrl).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  });

  test('GET /health returns 200 and {status: ok}', async () => {
    const res = await fetch(`${harness.baseUrl}/health`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as { status: string };
    expect(body.status).toBe('ok');
  });

  test('GET /bogus-route returns 404', async () => {
    // Proves routes NOT declared go through the default Express 404 handler —
    // the in-process server exhibits the same routing behavior as the live one.
    const res = await fetch(`${harness.baseUrl}/__definitely_not_a_route__`);
    expect(res.status).toBe(404);
  });
});
