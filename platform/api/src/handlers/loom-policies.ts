/**
 * GET /api/loom/policies — all chorus:Policy instances with enforces edges (#2339).
 *
 * Returns policies sorted alphabetically by label, each with the list of
 * principles it operationalizes. Empty set is a valid state (200 + empty
 * array). Orphan policies (no `chorus:enforces` edge) render with empty
 * `enforces[]` — the page surfaces them explicitly.
 */

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

export interface SparqlPolicyBinding {
  policy?: { value: string };
  label?: { value: string };
  comment?: { value: string };
  surface?: { value: string };
  principle?: { value: string };
  principleLabel?: { value: string };
}

interface PolicyPrincipleRef {
  id: string;
  label: string;
  uri: string;
}

interface PolicyRow {
  id: string;
  label: string;
  comment: string;
  surface: string;
  enforces: PolicyPrincipleRef[];
  uri: string;
}

export interface SparqlPoliciesResult {
  results: { bindings: SparqlPolicyBinding[] };
}

export interface LoomPoliciesDeps {
  sparql: (query: string) => Promise<SparqlPoliciesResult>;
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

function buildPolicyRow(b: SparqlPolicyBinding, uri: string): PolicyRow {
  return {
    id: stripPrefix(uri),
    label: b.label?.value ?? '',
    comment: b.comment?.value ?? '',
    surface: b.surface?.value ?? '',
    enforces: [],
    uri,
  };
}

function addPrincipleIfMissing(row: PolicyRow, b: SparqlPolicyBinding): void {
  const principleUri = b.principle?.value;
  if (!principleUri) return;
  if (row.enforces.some((p) => p.uri === principleUri)) return;
  row.enforces.push({
    id: stripPrefix(principleUri),
    label: b.principleLabel?.value ?? stripPrefix(principleUri),
    uri: principleUri,
  });
}

function foldBindings(bindings: SparqlPolicyBinding[]): PolicyRow[] {
  const byUri = new Map<string, PolicyRow>();
  for (const b of bindings) {
    const uri = b.policy?.value ?? '';
    if (!uri) continue;
    let row = byUri.get(uri);
    if (!row) {
      row = buildPolicyRow(b, uri);
      byUri.set(uri, row);
    }
    addPrincipleIfMissing(row, b);
  }
  const policies = Array.from(byUri.values());
  policies.sort((a, b) => a.label.localeCompare(b.label));
  return policies;
}

export async function fetchLoomPolicies(
  deps: LoomPoliciesDeps,
): Promise<{ status: number; body: unknown }> {
  const now = deps.now ?? (() => Date.now());
  const envelope = deps.envelope ?? defaultEnvelope;
  const started = now();

  try {
    const query = deps.loadQuery('loom-policies');
    const res = await deps.sparql(query);
    const policies = foldBindings(res.results?.bindings ?? []);
    const orphans = policies.filter((p) => p.enforces.length === 0).length;
    const durationMs = now() - started;
    return {
      status: 200,
      body: envelope('policies', { policies }, durationMs, {
        count: policies.length,
        orphan_count: orphans,
      }),
    };
  } catch (err) {
    const durationMs = now() - started;
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('policies', { error: message }, durationMs, { error: true }),
    };
  }
}
