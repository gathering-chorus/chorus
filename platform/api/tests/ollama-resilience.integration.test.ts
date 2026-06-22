/**
 * @test-type: api
 *
 * Ollama resilience tests — #1980
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Verifies the embed endpoint + health surface behave correctly.
 *
 * #3559: realigned to the CURRENT contract. #1978 moved embedding out of the
 * API process — POST /api/chorus/embed no longer runs synchronously and no
 * longer returns { embedded, skipped, ollama_failures }; it SPAWNS the embed
 * worker and returns 202 { status: "spawned", workers: [...] }. Ollama-failure
 * tracking now lives inside the worker, not the HTTP response, so the old
 * "response includes ollama_failures" assertion tested behavior that no longer
 * exists and was removed. The remaining tests assert the real current contract.
 */

import { startTestApp, type TestApp } from './lib/test-app';

const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';

describe('Ollama resilience — embed worker (#1980)', () => {

  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });
  test('Ollama is reachable (precondition) — skips if Ollama is down', async () => {
    // #3559 (Silas's call): Ollama is a separate CI on Bedroom with independent
    // reachability and is NOT part of the #3557 _stack_up probe (3340 + 3030).
    // So skip-if-unreachable PER-PRECONDITION here — a test that needs Ollama
    // skips when Ollama specifically is down, rather than false-redding a
    // stack-up nightly. (Probe-level refinement is owned by Silas, carded.)
    let res: Response;
    try {
      res = await fetch(`${OLLAMA_URL}/api/tags`);
    } catch {
      console.warn(`[ollama-resilience] Ollama unreachable at ${OLLAMA_URL} — skipping precondition`);
      return;
    }
    expect(res.status).toBe(200);
  });

  test('POST /api/chorus/embed spawns the embed worker (202)', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/embed`, { method: 'POST' });
    expect(res.status).toBe(202);
    const body = await res.json();
    expect(body.status).toBe('spawned');
    expect(Array.isArray(body.workers)).toBe(true);
    expect(body.workers).toContain('embed');
  }, 30_000);

  test('health detail exposes Ollama status', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/health/detail`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('ollama');
    expect(body.ollama).toHaveProperty('status');
  });
});
