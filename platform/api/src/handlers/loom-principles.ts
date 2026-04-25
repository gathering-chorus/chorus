/**
 * GET /api/loom/principles — all chorus:Principle instances (#2337).
 *
 * Thin product-facing endpoint. Returns principles sorted alphabetically by
 * rdfs:label, with { id, label, comment, uri } per instance. Empty set is a
 * valid state (returns 200 with empty array) — principles are added over
 * time, absence isn't a 404-worthy condition.
 *
 * The sibling endpoint `GET /api/athena/subdomains/loom-principles` already
 * returns this data nested inside a sub-domain envelope. This handler is a
 * narrower, loom-scoped alias — hides the subdomain-layout detail from any
 * caller that just wants "give me the principles."
 */

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

export interface SparqlPrincipleBinding {
  principle?: { value: string };
  label?: { value: string };
  comment?: { value: string };
  techReading?: { value: string };
  jeffReading?: { value: string };
  isPermacultureParent?: { value: string };
  order?: { value: string };
  parent?: { value: string };
  parentLabel?: { value: string };
}

interface PrincipleParentRef {
  id: string;
  label: string;
  uri: string;
}

interface PrincipleRow {
  id: string;
  label: string;
  comment: string;
  techReading: string;
  jeffReading: string;
  isPermacultureParent: boolean;
  order: number | null;
  parents: PrincipleParentRef[];
  uri: string;
}

export interface SparqlPrinciplesResult {
  results?: { bindings?: SparqlPrincipleBinding[] };
}

export interface LoomPrinciplesDeps {
  sparql: (query: string) => Promise<SparqlPrinciplesResult>;
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
  const slashIdx = uri.lastIndexOf('/');
  return slashIdx >= 0 ? uri.slice(slashIdx + 1) : uri;
}

function buildPrincipleRow(b: SparqlPrincipleBinding, uri: string): PrincipleRow {
  return {
    id: stripPrefix(uri),
    label: b.label?.value ?? '',
    comment: b.comment?.value ?? '',
    techReading: b.techReading?.value ?? '',
    jeffReading: b.jeffReading?.value ?? '',
    isPermacultureParent: b.isPermacultureParent?.value === 'true',
    order: b.order?.value ? Number.parseInt(b.order.value, 10) : null,
    parents: [],
    uri,
  };
}

function addParentIfMissing(row: PrincipleRow, b: SparqlPrincipleBinding): void {
  const parentUri = b.parent?.value;
  if (!parentUri) return;
  if (row.parents.some((p) => p.uri === parentUri)) return;
  row.parents.push({
    id: stripPrefix(parentUri),
    label: b.parentLabel?.value ?? stripPrefix(parentUri),
    uri: parentUri,
  });
}

function foldBindings(bindings: SparqlPrincipleBinding[]): PrincipleRow[] {
  // A principle can have multiple parents (services-R-B-I maps to two
  // permaculture parents) — bindings arrive one row per parent. Fold into
  // one row per principle URI, collecting parents[] as a set.
  const byUri = new Map<string, PrincipleRow>();
  for (const b of bindings) {
    const uri = b.principle?.value ?? '';
    if (!uri) continue;
    let row = byUri.get(uri);
    if (!row) {
      row = buildPrincipleRow(b, uri);
      byUri.set(uri, row);
    }
    addParentIfMissing(row, b);
  }
  const principles = Array.from(byUri.values());
  // Permaculture parents render in book order (chorus:order). Specializations and
  // unparented entries fall back to label sort. Frontend may re-group nested under
  // parents — this sort produces a deterministic flat order regardless.
  principles.sort((a, b) => {
    if (a.isPermacultureParent && b.isPermacultureParent) {
      return (a.order ?? 999) - (b.order ?? 999);
    }
    if (a.isPermacultureParent !== b.isPermacultureParent) {
      return a.isPermacultureParent ? -1 : 1;
    }
    return a.label.localeCompare(b.label);
  });
  return principles;
}

export async function fetchLoomPrinciples(
  deps: LoomPrinciplesDeps,
): Promise<{ status: number; body: unknown }> {
  const now = deps.now ?? (() => Date.now());
  const envelope = deps.envelope ?? defaultEnvelope;
  const started = now();

  try {
    const query = deps.loadQuery('loom-principles');
    const res = await deps.sparql(query);
    const principles = foldBindings(res.results?.bindings ?? []);
    const durationMs = now() - started;
    return {
      status: 200,
      body: envelope('principles', { principles }, durationMs, { count: principles.length }),
    };
  } catch (err) {
    const durationMs = now() - started;
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('principles', { error: message }, durationMs, { error: true }),
    };
  }
}
