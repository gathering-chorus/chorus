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

// #3749 — the SPARQL fold path (stripPrefix/buildPrincipleRow/addParentIfMissing/
// foldBindings + loom-principles.sparql) is RETIRED: it read urn:chorus:instances
// while the canonical principles lived elsewhere (the 2-of-29 split). The
// handler below sources the generated athena-make surface instead.

export interface LoomPrinciplesOwlDeps {
  /** injectable for tests — defaults to global fetch against the local athena-make */
  fetchFn?: typeof fetch;
  owlApiUrl?: string;
  now?: () => number;
  envelope?: typeof defaultEnvelope;
}

// #3749 — REPOINTED to the generated surface. The old implementation ran
// loom-principles.sparql against urn:chorus:instances while 27 of 29 canonical
// principles sat in urn:chorus:ontology — the writer/reader graph split that
// served 2-of-29 for months. athena-make /principles reads the shape-declared
// instance graph (urn:chorus:domains:principles, ADR-051), so this handler now
// has ONE source of truth and cannot disagree with the model. The legacy
// envelope shape (id/label/comment/readings/parents) is preserved so
// loom/principles.html and the session-boot injector keep working unchanged;
// `parents` is empty by construction post-#3749 (the 14 are peers).
export async function fetchLoomPrinciples(
  deps: LoomPrinciplesOwlDeps = {},
): Promise<{ status: number; body: unknown }> {
  const now = deps.now ?? (() => Date.now());
  const envelope = deps.envelope ?? defaultEnvelope;
  const fetchFn = deps.fetchFn ?? fetch;
  const base = deps.owlApiUrl ?? process.env.OWL_UPSTREAM ?? 'http://127.0.0.1:3360';
  const started = now();
  try {
    const listRes = await fetchFn(`${base}/principles`);
    if (!listRes.ok) {
      // an unreachable generated surface must never read as "no principles"
      return {
        status: 502,
        body: envelope('principles', { error: `athena-make /principles answered ${listRes.status}` }, now() - started, { error: true }),
      };
    }
    const list = (await listRes.json()) as { data?: Array<{ name?: string }> };
    const names = (list.data ?? []).map((r) => r.name ?? '').filter(Boolean);
    const principles: PrincipleRow[] = [];
    for (const name of names) {
      const entRes = await fetchFn(`${base}/principles/${encodeURIComponent(name)}`);
      if (!entRes.ok) continue; // a vanished entity mid-walk: skip, count tells
      // Partial<...>: the wire can omit any field — the type must admit that or the
      // ?? fallbacks below read as dead code to the linter while guarding real gaps (#3766).
      const ent = (await entRes.json()) as { data?: Partial<Record<string, string>> };
      const d = ent.data ?? {};
      principles.push({
        id: name,
        label: d.label ?? '',
        comment: d.comment ?? '',
        techReading: d.techReading ?? '',
        jeffReading: d.jeffReading ?? '',
        isPermacultureParent: d.isPermacultureParent === 'true',
        order: d.order ? Number.parseInt(d.order, 10) : null,
        parents: [],
        uri: d.iri ?? `${CHORUS_PREFIX}${name}`,
      });
    }
    // #4110 — a walk that loses EVERY row is a broken read, not an empty set.
    // #3749 repointed this handler at the generated surface and guarded the
    // case where that surface is unreachable; the case it did not guard is the
    // surface answering fine while every entity under it 404s. The `continue`
    // above is right for one row that vanished mid-walk and wrong when the
    // collection named N and none resolved, because then the caller cannot
    // tell that from "there are no principles yet".
    //
    // That is what happened: athena-make's collection served 28 while every
    // entity read answered 404, and this endpoint returned 200 with an empty
    // array. Every session booted with no principles, and the only trace was a
    // session.principles.empty event carrying error_type=api_returned_empty_set
    // — which reads like a fact about the data rather than a broken read.
    // Measured 2026-09-05 06:31: 28 named, 0 resolved.
    if (names.length > 0 && principles.length === 0) {
      return {
        status: 502,
        body: envelope(
          'principles',
          { error: `athena-make named ${names.length} principle(s) but none could be read — entity reads are failing` },
          now() - started,
          { error: true, named: names.length, resolved: 0 },
        ),
      };
    }
    principles.sort((a, b) => (a.order ?? 999) - (b.order ?? 999) || a.label.localeCompare(b.label));
    const durationMs = now() - started;
    return {
      status: 200,
      body: envelope('principles', { principles }, durationMs, { count: principles.length, servedFrom: 'athena-make:/principles' }),
    };
  } catch (err) {
    const durationMs = now() - started;
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 502,
      body: envelope('principles', { error: message }, durationMs, { error: true }),
    };
  }
}
