/**
 * GET /api/athena/subdomains/:id/alerts — Alert rules matching the domain (#2187).
 *
 * Scans a directory of YAML alert files. Match rule: domain keyword
 * (after -domain/-service/-analytics strip) appears in the filename OR
 * anywhere in the file content (case-insensitive). Fields parsed via
 * regex from YAML-ish content.
 */
import type { FetchResult } from './codebase-topology';
import { resolveDomainIdentity } from './domain-identity';

export interface AthenaSubdomainAlertsDeps {
  listAlertFiles: () => string[];
  readAlertFile: (filename: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

interface AlertSummary {
  file: string;
  name: string;
  description: string;
  severity: string;
  schedule: string;
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function matchesTokens(content: string, file: string, tokens: string[]): boolean {
  const lower = content.toLowerCase();
  const fileLower = file.toLowerCase();
  return tokens.some((t) => lower.includes(t) || fileLower.includes(t));
}

function parseAlertSummary(file: string, content: string): AlertSummary {
  return {
    file,
    name: content.match(/^name:\s*(.+)/m)?.[1]?.trim() ?? file.replace(/\.yml$/, ''),
    description: content.match(/^description:\s*(.+)/m)?.[1]?.trim() ?? '',
    severity: content.match(/^severity:\s*(.+)/m)?.[1]?.trim() ?? 'unknown',
    schedule: content.match(/^schedule:\s*"?(.+?)"?\s*$/m)?.[1]?.trim() ?? '',
  };
}

function collectAlerts(deps: AthenaSubdomainAlertsDeps, tokens: string[]): AlertSummary[] {
  const alerts: AlertSummary[] = [];
  for (const file of deps.listAlertFiles()) {
    const content = deps.readAlertFile(file);
    if (!matchesTokens(content, file, tokens)) continue;
    alerts.push(parseAlertSummary(file, content));
  }
  return alerts;
}

export function fetchAthenaSubdomainAlerts(
  deps: AthenaSubdomainAlertsDeps,
  id: string,
): FetchResult {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  try {
    const identity = resolveDomainIdentity(id);
    const alerts = collectAlerts(deps, identity.alertFileTokens);
    return {
      status: 200,
      body: envelope(
        'subdomain-alerts',
        { subdomain: id, domainLabel: identity.primary, alerts },
        now() - start,
        { count: alerts.length },
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-alerts', { error: message }, now() - start, { error: true }),
    };
  }
}
