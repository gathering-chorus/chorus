/**
 * GET /api/loom/decisions — all chorus:Decision instances (#2485 Move 2, redux 2026-04-25).
 *
 * Reads the rich schema migrated from urn:chorus:decisions → urn:chorus:instances per
 * #2485 Move 1 redux (ADR-025 canonical instances graph). Predicates: rdfs:label (id),
 * rdfs:comment (title), chorus:decisionType (DEC|ADR|protocol), chorus:decisionStatus,
 * chorus:decisionDate, chorus:enforcementLevel, chorus:hasDomain (multi-edge).
 *
 * Folds bindings (one row per hasDomain edge) into one row per decision URI with
 * domains[] aggregated. Sort: ADR first, then DEC/protocol; within each, label desc.
 *
 * The sibling endpoint `GET /api/athena/subdomains/loom-decisions` returns
 * decisions nested inside a sub-domain envelope. This handler is the narrower
 * loom-scoped alias.
 */

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

export interface SparqlDecisionBinding {
  decision?: { value: string };
  id?: { value: string };
  title?: { value: string };
  decisionType?: { value: string };
  status?: { value: string };
  date?: { value: string };
  level?: { value: string };
  domain?: { value: string };
}

interface DecisionRow {
  uri: string;
  id: string;
  title: string;
  decisionType: string;
  status: string;
  date: string;
  enforcementLevel: string;
  domains: string[];
}

export interface SparqlDecisionsResult {
  results?: { bindings?: SparqlDecisionBinding[] };
}

export interface LoomDecisionsDeps {
  sparql: (query: string) => Promise<SparqlDecisionsResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'loom', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function stripPrefix(uri: string): string {
  if (uri.startsWith(CHORUS_PREFIX)) return uri.slice(CHORUS_PREFIX.length);
  const hashIdx = uri.lastIndexOf('#');
  if (hashIdx >= 0) return uri.slice(hashIdx + 1);
  const colonIdx = uri.lastIndexOf(':');
  if (colonIdx >= 0) return uri.slice(colonIdx + 1);
  const slashIdx = uri.lastIndexOf('/');
  return slashIdx >= 0 ? uri.slice(slashIdx + 1) : uri;
}

function buildDecisionRow(b: SparqlDecisionBinding, uri: string): DecisionRow {
  return {
    uri,
    id: b.id?.value ?? '',
    title: b.title?.value ?? '',
    decisionType: b.decisionType?.value ?? '',
    status: b.status?.value ?? '',
    date: b.date?.value ?? '',
    enforcementLevel: b.level?.value ?? '',
    domains: [],
  };
}

function addDomainIfMissing(row: DecisionRow, b: SparqlDecisionBinding): void {
  const domain = b.domain?.value;
  if (!domain) return;
  const slug = stripPrefix(domain);
  if (row.domains.includes(slug)) return;
  row.domains.push(slug);
}

function foldBindings(bindings: SparqlDecisionBinding[]): DecisionRow[] {
  // A decision can have multiple chorus:hasDomain edges — bindings arrive one row per
  // domain. Fold into one row per decision URI with domains[] aggregated.
  const byUri = new Map<string, DecisionRow>();
  for (const b of bindings) {
    const uri = b.decision?.value ?? '';
    if (!uri) continue;
    let row = byUri.get(uri);
    if (!row) {
      row = buildDecisionRow(b, uri);
      byUri.set(uri, row);
    }
    addDomainIfMissing(row, b);
  }
  const decisions = Array.from(byUri.values());
  decisions.sort((a, b) => {
    const ta = a.decisionType === 'ADR' ? 0 : 1;
    const tb = b.decisionType === 'ADR' ? 0 : 1;
    if (ta !== tb) return ta - tb;
    return b.id.localeCompare(a.id);
  });
  return decisions;
}

export async function fetchLoomDecisions(
  deps: LoomDecisionsDeps,
): Promise<{ status: number; body: unknown }> {
  const now = deps.now ?? (() => Date.now());
  const envelope = deps.envelope ?? defaultEnvelope;
  const started = now();

  try {
    const query = deps.loadQuery('loom-decisions');
    const res = await deps.sparql(query);
    const decisions = foldBindings(res.results?.bindings ?? []);
    const durationMs = now() - started;
    return {
      status: 200,
      body: envelope('decisions', { decisions }, durationMs, { count: decisions.length }),
    };
  } catch (err) {
    const durationMs = now() - started;
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('decisions', { error: message }, durationMs, { error: true }),
    };
  }
}
