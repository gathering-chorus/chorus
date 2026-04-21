/**
 * GET /api/chorus/context/spine?limit=N (#2252).
 *
 * Answers: "What's happened recently on the spine?" Returns the last N spine
 * events from chorus.log, newest first. Default 20, max 500.
 *
 * Source: platform/logs/chorus.log (JSON-lines). Each line is one event;
 * malformed lines are skipped silently (logs outrun schema changes).
 *
 * Scope: domain (chorus) — spine is a chorus-product surface. Envelope
 * carries step + product + domain; no subdomain (spine spans all subdomains).
 */

import {
  stampHeader,
  buildEnvelope,
  type StampSparqlClient,
  type ContextEnvelope,
} from '../lib/context-envelope';

export interface ContextSpineDeps {
  sparql: StampSparqlClient;
  /** Returns chorus.log contents, or null if missing. */
  readLog: () => string | null;
}

export interface SpineEventEntry {
  timestamp: string;
  event: string;
  role: string;
  card?: string;
  trace_id?: string;
}

export interface ContextSpineResponse {
  status: number;
  body: ContextEnvelope<{ total: number; limit: number; events: SpineEventEntry[] }> | { error: string };
}

const DEFAULT_LIMIT = 20;
const MAX_LIMIT = 500;

export async function fetchContextSpine(
  deps: ContextSpineDeps,
  sourceUrl: string,
  limitRaw?: string,
): Promise<ContextSpineResponse> {
  const raw = deps.readLog();
  if (raw === null) {
    return { status: 503, body: { error: 'No chorus.log available; spine state unknown.' } };
  }

  const parsed = parseInt(limitRaw ?? '', 10);
  const requested = Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_LIMIT;
  const limit = Math.min(requested, MAX_LIMIT);
  const events = parseTailEvents(raw, limit);
  const header = await stampHeader(deps.sparql, 'chorus');
  return {
    status: 200,
    body: buildEnvelope(header, sourceUrl, { total: events.length, limit, events }),
  };
}

function parseTailEvents(raw: string, limit: number): SpineEventEntry[] {
  const lines = raw.trim().split('\n');
  const out: SpineEventEntry[] = [];
  for (let i = lines.length - 1; i >= 0 && out.length < limit; i--) {
    const entry = parseLine(lines[i]);
    if (entry) out.push(entry);
  }
  return out;
}

function parseLine(line: string): SpineEventEntry | null {
  try {
    const parsed = JSON.parse(line);
    if (!parsed.event || !parsed.timestamp) return null;
    const entry: SpineEventEntry = {
      timestamp: String(parsed.timestamp),
      event: String(parsed.event),
      role: String(parsed.role || 'system'),
    };
    if (parsed.card) entry.card = String(parsed.card);
    if (parsed.trace_id) entry.trace_id = String(parsed.trace_id);
    return entry;
  } catch {
    return null;
  }
}
