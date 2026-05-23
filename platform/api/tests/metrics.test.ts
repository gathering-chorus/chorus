// #3039 — chorus-api had ZERO Prometheus instrumentation (/metrics = 404), so a
// blocked event loop (the March incident, and the 6.24s idle stall on the releases
// route) was invisible — inferred from symptoms, never measured. These tests pin
// the foundation: a shared registry exposing nodejs_eventloop_lag_seconds, and the
// guarantee that #2482's tool-call counters can register onto the SAME registry
// (one /metrics endpoint, one scrape target).
import client from 'prom-client';
import { startMetrics, getMetrics, registry } from '../src/metrics';

describe('#3039 chorus-api metrics foundation', () => {
  it('exposes Node event-loop lag — the signal missing since March', async () => {
    startMetrics();
    const { contentType, body } = await getMetrics();
    expect(contentType).toMatch(/text\/plain/); // Prometheus exposition format
    expect(body).toMatch(/nodejs_eventloop_lag_seconds/);
    expect(body).toMatch(/service="chorus-api"/);
  });

  it('shares one registry so #2482 tool-call counters land on the same endpoint', async () => {
    startMetrics();
    const c = new client.Counter({
      name: 'test_tool_call_total_3039',
      help: 'proves a counter on the shared registry surfaces at the one /metrics endpoint',
      registers: [registry],
    });
    c.inc();
    const { body } = await getMetrics();
    // default label service="chorus-api" sits between metric name and value
    expect(body).toMatch(/test_tool_call_total_3039(\{[^}]*\})? 1/);
  });

  it('startMetrics is idempotent — safe at boot and in tests', () => {
    expect(() => { startMetrics(); startMetrics(); }).not.toThrow();
  });
});
