// @test-type: unit
// #3580 — gate:quality dispatches checks by subdomain, consuming the generated
// tests-domain API (:3360/tests, landed #2819). These tests cover the PURE
// selection core: given a subdomain + a stubbed /tests response, the correct
// covering-test set is selected. Hermetic — no live API (AC3).
import {
  selectCoveringTests,
  dispatchBySubdomain,
  dispatchForCard,
  type TestsApiResponse,
} from '../src/gate-quality-tests-dispatch';

// A stubbed /tests payload shaped exactly like owl-api serves (the real keys:
// testName, covers, filePath, pyramidLayer). Three subdomains represented.
const STUB: TestsApiResponse = {
  count: 4,
  data: [
    { testName: 'logs the iso timestamp', covers: 'senses', filePath: 'a.test.ts', pyramidLayer: 'unit' },
    { testName: 'reclaims a stale lock', covers: 'werk', filePath: 'b.rs', pyramidLayer: 'unit' },
    { testName: 'alerts on red', covers: 'ops-alerts', filePath: 'c.rs', pyramidLayer: 'integration' },
    { testName: 'fires the second alert', covers: 'ops-alerts', filePath: 'c.rs', pyramidLayer: 'integration' },
  ],
};

describe('selectCoveringTests — join on covers→Domain (AC1)', () => {
  it('selects only the tests whose covers matches the subdomain', () => {
    const got = selectCoveringTests('ops-alerts', STUB);
    expect(got.map((t) => t.testName)).toEqual(['alerts on red', 'fires the second alert']);
  });

  it('returns empty for a subdomain no test covers', () => {
    expect(selectCoveringTests('photos', STUB)).toEqual([]);
  });

  it('returns empty (fail-open shape) for empty subdomain or empty data', () => {
    expect(selectCoveringTests('', STUB)).toEqual([]);
    expect(selectCoveringTests('werk', { data: [] })).toEqual([]);
  });
});

describe('dispatchBySubdomain — scoped, not whole-suite (AC2)', () => {
  it('a card in subdomain X gets X tests, a card in subdomain Y gets Y tests (different sets)', () => {
    const x = dispatchBySubdomain('senses', STUB);
    const y = dispatchBySubdomain('werk', STUB);
    expect(x.coveringTests).toEqual(['logs the iso timestamp']);
    expect(y.coveringTests).toEqual(['reclaims a stale lock']);
    expect(x.coveringTests).not.toEqual(y.coveringTests); // scoped, not blanket
  });

  it('carries a receipt: which subdomain was consulted + the count (AC4)', () => {
    const r = dispatchBySubdomain('ops-alerts', STUB);
    expect(r.subdomain).toBe('ops-alerts');
    expect(r.count).toBe(2);
    expect(r.scoped).toBe(true);
  });

  it('scoped=false when no covering tests — degrade, never block (AC5 shape)', () => {
    const r = dispatchBySubdomain('photos', STUB);
    expect(r.count).toBe(0);
    expect(r.scoped).toBe(false);
  });
});

describe('dispatchForCard — live fetch wiring, fail-open (AC1 live + AC5)', () => {
  const okFetch = async () =>
    ({ ok: true, json: async () => STUB }) as unknown as Response;

  it('reads the tests API and returns the scoped dispatch', async () => {
    const r = await dispatchForCard('ops-alerts', { fetchImpl: okFetch });
    expect(r.count).toBe(2);
    expect(r.scoped).toBe(true);
    expect(r.coveringTests).toEqual(['alerts on red', 'fires the second alert']);
  });

  it('fails OPEN when the API is down (fetch throws) — degrades, never throws', async () => {
    const downFetch = async () => {
      throw new Error('ECONNREFUSED');
    };
    const r = await dispatchForCard('ops-alerts', { fetchImpl: downFetch });
    expect(r.scoped).toBe(false);
    expect(r.count).toBe(0);
    expect(r.subdomain).toBe('ops-alerts'); // receipt still names what it tried
  });

  it('fails OPEN on a non-2xx response (e.g. 500)', async () => {
    const badFetch = async () => ({ ok: false, status: 500 }) as unknown as Response;
    const r = await dispatchForCard('werk', { fetchImpl: badFetch });
    expect(r.scoped).toBe(false);
  });
});
