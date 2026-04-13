/**
 * Ollama resilience tests — #1980
 *
 * Integration tests — hit live Chorus API at localhost:3340.
 * Verifies that the embed pipeline handles Ollama failures gracefully.
 *
 * Prior work: #1978 moved embed out of API process. embedQuery() has no retry —
 * single call, 15s timeout, failures silently skipped. No availability tracking.
 * Approach: retry with backoff in embedQuery(), expose ollama_failures count,
 * add Ollama status to health/detail endpoint.
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://127.0.0.1:11434';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('Ollama resilience — embed worker (#1980)', () => {
  test('Ollama is reachable (precondition)', async () => {
    const res = await fetch(`${OLLAMA_URL}/api/tags`);
    expect(res.status).toBe(200);
  });

  test('POST /api/chorus/embed succeeds when Ollama is up', async () => {
    const res = await fetch(`${API}/api/chorus/embed`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(typeof body.embedded).toBe('number');
    expect(typeof body.skipped).toBe('number');
  }, 30_000);

  test('embed response includes ollama_failures for availability tracking', async () => {
    const res = await fetch(`${API}/api/chorus/embed`, { method: 'POST' });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('ollama_failures');
    expect(typeof body.ollama_failures).toBe('number');
  }, 30_000);

  test('health detail exposes Ollama status', async () => {
    const res = await fetch(`${API}/api/chorus/health/detail`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('ollama');
    expect(body.ollama).toHaveProperty('status');
  });
});
