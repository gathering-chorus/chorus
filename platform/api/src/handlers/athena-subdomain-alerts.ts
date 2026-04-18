/**
 * GET /api/athena/subdomains/:id/alerts — Alert rules matching the domain (#2187).
 *
 * Scans a directory of YAML alert files. Match rule: domain keyword
 * (after -domain/-service/-analytics strip) appears in the filename OR
 * anywhere in the file content (case-insensitive). Fields parsed via
 * regex from YAML-ish content.
 */
import type { FetchResult } from './codebase-topology';

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

export async function fetchAthenaSubdomainAlerts(
  deps: AthenaSubdomainAlertsDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();

  try {
    const domainLabel = id.replace(/-(?:domain|service|analytics)$/, '').toLowerCase();
    const files = deps.listAlertFiles();
    const alerts: AlertSummary[] = [];
    for (const file of files) {
      const content = deps.readAlertFile(file);
      const lower = content.toLowerCase();
      if (!lower.includes(domainLabel) && !file.toLowerCase().includes(domainLabel)) continue;
      const name = content.match(/^name:\s*(.+)/m)?.[1]?.trim() ?? file.replace(/\.yml$/, '');
      const description = content.match(/^description:\s*(.+)/m)?.[1]?.trim() ?? '';
      const severity = content.match(/^severity:\s*(.+)/m)?.[1]?.trim() ?? 'unknown';
      const schedule = content.match(/^schedule:\s*"?(.+?)"?\s*$/m)?.[1]?.trim() ?? '';
      alerts.push({ file, name, description, severity, schedule });
    }
    return {
      status: 200,
      body: envelope(
        'subdomain-alerts',
        { subdomain: id, domainLabel, alerts },
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
