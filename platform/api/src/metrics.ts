// #3039 — chorus-api event-loop / latency observability foundation.
//
// The most load-bearing process in the system (every role's MCP / cards / search
// / athena / nudge routes through chorus-api) had ZERO Prometheus instrumentation
// — /metrics was 404. March's event-loop blocking went unalerted because the lag
// metric was never scraped. This module is the foundation: ONE shared registry
// exposing Node's default metrics, including nodejs_eventloop_lag_seconds — the
// signal that lets us SEE a blocked loop instead of inferring it from a stall.
//
// #2482 builds on this: its MCP tool-call counters (tool_call_total, _duration,
// mcp_sessions_open) and adoption-curve panels register onto THIS registry, so
// there is one /metrics endpoint and one scrape target, never two.
import client from 'prom-client';

/** The single chorus-api metrics registry. #2482 registers its counters here. */
export const registry = new client.Registry();
registry.setDefaultLabels({ service: 'chorus-api' });

// #3628 — refusals as a scrapeable signal. When the envelope refused a real
// caller for hours (the stale tokenless SDK dist: 4354 refusals/24h on
// /api/chorus/trace), nothing measured it — the gap was found by eyeballing
// Loki after Jeff asked. This counter feeds the Prometheus alert
// (envelope-refusal-alerts.yml, #3625 nudge path) that fires on a sustained
// refused-rate regression instead.
const envelopeRefusals = new client.Counter({
  name: 'chorus_envelope_refusals_total',
  help: 'security.envelope.refused events by surface and reason (#3628)',
  labelNames: ['surface', 'reason'],
  registers: [registry],
});

/** Mirror an envelope spine event into Prometheus. Only refusals count;
 *  attempt/allowed pass through untouched. Never throws — the envelope's
 *  emit path must stay fire-and-forget. */
export function recordEnvelopeEvent(event: string, fields: Record<string, string>): void {
  if (event !== 'security.envelope.refused') return;
  envelopeRefusals.inc({
    surface: fields.surface || 'unknown',
    reason: fields.reason || 'unknown',
  });
}

let started = false;

/** Idempotent: begin collecting Node default metrics (event-loop lag, heap, GC,
 *  handles) into the shared registry. Safe to call once at boot and again in tests. */
export function startMetrics(): void {
  if (started) return;
  started = true;
  client.collectDefaultMetrics({ register: registry });
}

/** Render the registry in Prometheus text exposition format for the /metrics route. */
export async function getMetrics(): Promise<{ contentType: string; body: string }> {
  return { contentType: registry.contentType, body: await registry.metrics() };
}
