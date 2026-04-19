/**
 * Context envelope — canonical-metadata-stamped response wrapper.
 *
 * Every `/api/chorus/context/*` endpoint returns a body with this shape.
 * Header fields place the response in the canonical model so agents cite it
 * unambiguously; `data` carries the endpoint-specific payload.
 *
 * Canonical metadata source: Athena named graph in Fuseki `/pods`. Same
 * graph every Athena handler already queries. NOT the DOMAIN_REGISTRY TS
 * object in server.ts — that would split ownership of data whose home is
 * OWL/RDF. Resolving via SPARQL keeps one source of truth.
 *
 * See `designing/docs/context-endpoint-schemas.md` (#2234 Step 2) for the
 * authoritative shape. This file is the Step 3 implementation.
 */

export const ATHENA_GRAPH = 'urn:chorus:ontology';

/** Minimal SPARQL client surface — same shape as AthenaSparqlClient.query. */
export interface StampSparqlClient {
  query(query: string): Promise<SparqlSelectResult>;
}

/** SELECT result shape returned by the Fuseki JSON-results endpoint. */
export interface SparqlSelectResult {
  results?: { bindings?: Array<Record<string, { value: string } | undefined>> };
}

export interface ContextEnvelope<T = unknown> {
  step?: string;
  product?: string;
  domain?: string;
  subdomain?: string;
  timestamp: string;
  source: string;
  data: T;
}

/**
 * Fields produced by stampHeader — everything except `source` and `data`,
 * which the handler adds. `timestamp` is always set.
 */
export interface ContextEnvelopeHeader {
  step?: string;
  product?: string;
  domain?: string;
  subdomain?: string;
  timestamp: string;
}

/**
 * Resolve canonical-model fields (step, product, domain, subdomain) from
 * the Athena named graph via SPARQL.
 *
 * - `domainId = null` (system-scoped) → returns `{ timestamp }` only.
 * - `domainId` given, no graph match → returns `{ domain, timestamp, subdomain? }`
 *   (step + product gracefully absent, not null, not a crash).
 * - `domainId` given, graph match → returns `{ step, product, domain, subdomain?, timestamp }`.
 *
 * Tests inject a stub `client` whose `query()` returns fixture bindings.
 */
export async function stampHeader(
  client: StampSparqlClient,
  domainId: string | null,
  subdomainId?: string | null,
): Promise<ContextEnvelopeHeader> {
  const timestamp = new Date().toISOString();
  if (!domainId) {
    return { timestamp };
  }

  // Single LIMIT 1 SELECT — cheap, cacheable. One place for the query to
  // live; every Context handler calls this one function.
  const sparql = `
    PREFIX chorus: <urn:gathering:chorus#>
    SELECT ?product ?step WHERE {
      GRAPH <${ATHENA_GRAPH}> {
        ?d chorus:name "${escapeLiteral(domainId)}" ;
           chorus:product ?product ;
           chorus:step    ?step .
      }
    } LIMIT 1
  `;

  let step: string | undefined;
  let product: string | undefined;
  try {
    const result = await client.query(sparql);
    const b = result?.results?.bindings?.[0];
    step = b?.step?.value;
    product = b?.product?.value;
  } catch {
    // SPARQL failure → graceful-absent. Consumer still gets `domain` +
    // `timestamp`; callers monitoring Athena liveness detect via other signals.
  }

  return {
    ...(step && { step }),
    ...(product && { product }),
    domain: domainId,
    ...(subdomainId && { subdomain: subdomainId }),
    timestamp,
  };
}

/**
 * Compose a full envelope. `data` is endpoint-specific; everything else is
 * derived from the header + request URL.
 */
export function buildEnvelope<T>(
  header: ContextEnvelopeHeader,
  source: string,
  data: T,
): ContextEnvelope<T> {
  return {
    ...header,
    source,
    data,
  };
}

/** Escape double-quote in a literal we inline into SPARQL. Domain ids are
 *  ASCII identifiers today, but cheap to be safe. */
function escapeLiteral(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}
