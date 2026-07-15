// @test-type: unit — signal:security is fixture-data (tests the metrics counter pure, no live envelope)
// #3628 — the envelope refuses real callers silently: when /api/chorus/trace
// refused 4354 calls/24h (a stale tokenless SDK dist), nothing measured it —
// Silas found it by eyeballing Loki after Jeff asked. These tests pin the
// counter that makes refusals a scrapeable signal: every
// security.envelope.refused event increments chorus_envelope_refusals_total
// so the Prometheus alert (envelope-refusal-alerts.yml, same #3625 nudge
// path) can fire on a sustained refused-rate regression.
import { recordEnvelopeEvent, getMetrics, registry } from '../src/metrics';

function counterValue(body: string, labels: string): number {
  // exposition line: chorus_envelope_refusals_total{...labels...} <value>
  const re = new RegExp(`chorus_envelope_refusals_total\\{[^}]*${labels}[^}]*\\} (\\d+)`);
  const m = re.exec(body);
  return m ? parseInt(m[1], 10) : 0;
}

describe('#3628 envelope refusal counter', () => {
  it('a refused event increments the counter with surface + reason labels', async () => {
    recordEnvelopeEvent('security.envelope.refused', {
      surface: 'surface-post-api-chorus-trace',
      path: '/api/chorus/trace',
      reason: 'authn-missing',
    });
    const { body } = await getMetrics();
    expect(
      counterValue(body, 'surface="surface-post-api-chorus-trace"'),
    ).toBeGreaterThanOrEqual(1);
    expect(body).toMatch(/reason="authn-missing"/);
  });

  it('attempt and allowed events do NOT increment the refusal counter', async () => {
    const { body: before } = await getMetrics();
    const beforeVal = counterValue(before, 'surface="surface-post-api-chorus-cards"');
    recordEnvelopeEvent('security.envelope.attempt', {
      surface: 'surface-post-api-chorus-cards', path: '/api/chorus/cards',
    });
    recordEnvelopeEvent('security.envelope.allowed', {
      surface: 'surface-post-api-chorus-cards', path: '/api/chorus/cards',
      webId: 'http://localhost:3000/pods/chorus/_agents/chorus-sdk/profile/card.ttl#me',
    });
    const { body: after } = await getMetrics();
    expect(counterValue(after, 'surface="surface-post-api-chorus-cards"')).toBe(beforeVal);
  });

  it('missing surface/reason fields land as "unknown", never a throw', async () => {
    expect(() => recordEnvelopeEvent('security.envelope.refused', {})).not.toThrow();
    const { body } = await getMetrics();
    expect(counterValue(body, 'surface="unknown"')).toBeGreaterThanOrEqual(1);
  });

  it('the counter lives on the shared registry (one /metrics, one scrape target)', async () => {
    const metric = registry.getSingleMetric('chorus_envelope_refusals_total');
    expect(metric).toBeDefined();
  });
});
