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
 * derivation, applied to the live proxy data, yields > 0 rendered rows. Empty data =
 * FAIL, never skip (#3190: a test that passes on zero rows is the bug).
 *
 * Integration — requires RUN_INTEGRATION=true + chorus-api on 3340 (proxying owl-api).
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

let apiUp = false;

beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    const res = await fetch(`${API}/api/athena/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('value-stream.html renders rows from the live /owl proxy (#3701)', () => {
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
