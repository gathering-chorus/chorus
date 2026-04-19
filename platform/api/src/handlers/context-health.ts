/**
 * GET /api/chorus/context/health (#2234 Step 3).
 *
 * Answers: "Is anything broken or degraded right now?" Returns a top-level
 * status (ok | degraded | down) plus failure/warning counts and a per-check
 * breakdown.
 *
 * Source today: pulse-latest.json's `health` block — mirrored from
 * deep-health.sh output by the pulse daemon (#1881). A later card can swap
 * DI to read deep-health directly.
 *
 * Scope: system — no domain. The envelope carries timestamp + source + data
 * only. Health spans the whole product, not one domain within it.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextHealthDeps {
  sparql: StampSparqlClient;
  readPulse: () => string | null;
}

export type HealthStatus = 'ok' | 'warning' | 'degraded' | 'down';
export type CheckStatus = 'ok' | 'warning' | 'error' | 'unknown';

export interface HealthCheck {
  name: string;
  status: CheckStatus;
  detail?: string;
  latencyMs?: number;
  lastCheck?: string;
}

export interface ContextHealthData {
  status: HealthStatus;
  failures: number;
  warnings: number;
  summary: string;
  checks: HealthCheck[];
}

export interface ContextHealthResponse {
  status: number;
  body: ContextEnvelope<ContextHealthData> | { error: string };
}

export async function fetchContextHealth(
  deps: ContextHealthDeps,
  sourceUrl: string,
): Promise<ContextHealthResponse> {
  const raw = deps.readPulse();
  if (raw === null) {
    return { status: 503, body: { error: 'No pulse snapshot available; health unknown.' } };
  }
  let pulse: unknown;
  try {
    pulse = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `pulse-latest.json unparseable: ${message}` } };
  }

  const header = await stampHeader(deps.sparql, null);

  const healthRaw = (pulse as { health?: Record<string, unknown> })?.health ?? {};
  const status = toHealthStatus(healthRaw.status);
  const failures = numericOr(healthRaw.failures, 0);
  const warnings = numericOr(healthRaw.warning_count ?? healthRaw.warnings, 0);
  const summary = typeof healthRaw.summary === 'string' ? healthRaw.summary : '';
  const checks = Array.isArray(healthRaw.checks)
    ? healthRaw.checks
        .filter((c: unknown): c is Record<string, unknown> => !!c && typeof c === 'object')
        .map(shapeCheck)
    : [];

  const data: ContextHealthData = { status, failures, warnings, summary, checks };
  return { status: 200, body: buildEnvelope(header, sourceUrl, data) };
}

function toHealthStatus(raw: unknown): HealthStatus {
  if (raw === 'ok' || raw === 'warning' || raw === 'degraded' || raw === 'down') return raw;
  return 'ok';
}

function numericOr(raw: unknown, fallback: number): number {
  return typeof raw === 'number' && Number.isFinite(raw) ? raw : fallback;
}

function shapeCheck(raw: Record<string, unknown>): HealthCheck {
  const str = (k: string): string | undefined =>
    typeof raw[k] === 'string' ? (raw[k] as string) : undefined;
  const status = (raw.status === 'ok' || raw.status === 'warning' || raw.status === 'error')
    ? raw.status
    : 'unknown';
  const check: HealthCheck = {
    name: str('name') ?? 'unknown',
    status,
  };
  const detail = str('detail');
  if (detail) check.detail = detail;
  const last = str('lastCheck') ?? str('last_check');
  if (last) check.lastCheck = last;
  if (typeof raw.latencyMs === 'number') check.latencyMs = raw.latencyMs;
  else if (typeof raw.latency_ms === 'number') check.latencyMs = raw.latency_ms;
  return check;
}
