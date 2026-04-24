/**
 * GET /api/chorus/context/alerts (#2252).
 *
 * Answers: "What alerts are firing right now?" Reads `alerts.fired_today`
 * from pulse-latest.json (the same daemon mirror every other context handler
 * already reads) and enriches each with severity + description from the
 * authoritative yaml in proving/domains/alerts/.
 *
 * Scope: domain (chorus) — alerts are a chorus-product observability surface.
 * A per-domain variant can be added later; out of scope here.
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextAlertsDeps {
  sparql: StampSparqlClient;
  readPulse: () => string | null;
  listAlertFiles: () => string[];
  readAlertFile: (name: string) => string | null;
}

export interface AlertEntry {
  name: string;
  severity: string;
  description: string;
}

export interface ContextAlertsResponse {
  status: number;
  body: ContextEnvelope<{ total: number; alerts: AlertEntry[] }> | { error: string };
}

export async function fetchContextAlerts(
  deps: ContextAlertsDeps,
  sourceUrl: string,
): Promise<ContextAlertsResponse> {
  const raw = deps.readPulse();
  if (raw === null) {
    return { status: 503, body: { error: 'No pulse snapshot available; alert state unknown.' } };
  }
  let pulse: unknown;
  try {
    pulse = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { status: 500, body: { error: `pulse-latest.json unparseable: ${message}` } };
  }

  const fired = readFiredList(pulse);
  const metaByName = buildAlertMetadata(deps);
  const alerts: AlertEntry[] = fired.map((name) => metaByName.get(name) ?? { name, severity: 'unknown', description: '' });

  const header = await stampHeader(deps.sparql, 'chorus');
  return {
    status: 200,
    body: buildEnvelope(header, sourceUrl, { total: alerts.length, alerts }),
  };
}

function readFiredList(pulse: unknown): string[] {
  const block = (pulse as { alerts?: { fired_today?: unknown[] } }).alerts;
  if (!block || !Array.isArray(block.fired_today)) return [];
  return block.fired_today.filter((n): n is string => typeof n === 'string');
}

function buildAlertMetadata(deps: ContextAlertsDeps): Map<string, AlertEntry> {
  const out = new Map<string, AlertEntry>();
  for (const file of deps.listAlertFiles()) {
    if (!file.endsWith('.yml') && !file.endsWith('.yaml')) continue;
    const content = deps.readAlertFile(file);
    if (!content) continue;
    const entry = parseAlertYaml(content);
    if (entry) out.set(entry.name, entry);
  }
  return out;
}

const NAME_RE = /^name:\s*(.+)$/m;
const SEVERITY_RE = /^severity:\s*(.+)$/m;
const DESCRIPTION_RE = /^description:\s*(.+)$/m;

function parseAlertYaml(content: string): AlertEntry | null {
  const name = content.match(NAME_RE)?.[1].trim();
  if (!name) return null;
  return {
    name,
    severity: content.match(SEVERITY_RE)?.[1].trim() ?? 'unknown',
    description: content.match(DESCRIPTION_RE)?.[1].trim() ?? '',
  };
}
