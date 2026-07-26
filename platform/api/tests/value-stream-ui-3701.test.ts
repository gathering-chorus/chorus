/**
 * @test-type: api
 *
 * #3701 AC2 — value-stream.html renders rows > 0 against the live /owl proxy.
 *
 * The page (athena/value-stream.html) fetches /owl/valuestreams + /owl/valuestreamsteps
 * and renders one step-column per Chorus step (inStream includes 'chorus', sorted by
 * stageOrder). No browser runtime in this suite (testEnvironment: node, no jsdom dep —
 * a branch-only dep reds werk-build), so this test proves the same thing at the seam
 * the page rides: the served HTML wires those fetches, and the page's own row
 * derivation, applied to the live proxy data, yields > 0 rendered rows.
 *
 * Gating (#3701 anti-false-green, the #3190 rule): NO env flag. A probe at module
 * load decides — chorus-api UNREACHABLE → skip (env absence, #3528 contract);
 * chorus-api REACHABLE with empty data → FAIL. The suite's RUN_INTEGRATION
 * convention is deliberately not used here: the werk pipeline doesn't set it, so
 * an env-gated version silently skips exactly where this card promises red
 * (gate:code + gate:arch findings, round 1fc11eaa3c70).
 */
import { execSync } from 'child_process';

const API = process.env.CHORUS_API || 'http://localhost:3340';

// Synchronous reachability probe at module load — jest needs the describe/skip
// decision before any async runs. curl matches the repo's zero-dep probe idiom.
function apiReachable(): boolean {
  try {
    const code = execSync(
      `curl -s -o /dev/null -w '%{http_code}' --max-time 3 ${API}/api/athena/health`,
      { encoding: 'utf8', timeout: 5000 }
    ).trim();
    return code === '200';
  } catch {
    return false;
  }
}

const up = apiReachable();
const describeLive = up ? describe : describe.skip;
if (!up) {
  // visible skip reason — absence of the service, never absence of rows
  console.warn(`value-stream-ui-3701: chorus-api not reachable at ${API} — skipping (env absence)`);
}

describeLive('value-stream.html renders rows from the live /owl proxy (#3701)', () => {
  test('the page is served and wires the /owl fetches it renders from', async () => {
    const res = await fetch(`${API}/athena/value-stream.html`);
    expect(res.status).toBe(200);
    const html = await res.text();
    expect(html).toContain("fetchJSON('/valuestreams')");
    expect(html).toContain("fetchJSON('/valuestreamsteps')");
    expect(html).toContain('id="main"');
  });

  test('/owl/valuestreams serves >= 1 stream — 0 rows = red, not skip', async () => {
    const res = await fetch(`${API}/owl/valuestreams`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.count).toBeGreaterThanOrEqual(1);
    expect(Array.isArray(body.data)).toBe(true);
    expect(body.data.length).toBeGreaterThanOrEqual(1);
  });

  test('the page row derivation yields > 0 rendered step columns', async () => {
    const res = await fetch(`${API}/owl/valuestreamsteps`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const steps: Array<{ inStream?: unknown; stageOrder?: unknown }> = body.data || [];
    // exactly the page's derivation (value-stream.html): the ordered Chorus steps
    // are the rendered columns — zero ordered steps = a blank stream on screen
    const ordered = steps
      .filter((s) => s.inStream && String(s.inStream).includes('chorus'))
      .sort((a, b) => Number(a.stageOrder || 0) - Number(b.stageOrder || 0));
    expect(ordered.length).toBeGreaterThan(0);
  });
});
