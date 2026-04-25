/* eslint-disable security/detect-object-injection -- Hierarchy indexed by SPARQL-bound IDs. */
/**
 * GET /api/chorus/products — Product → SubProduct → SubDomain hierarchy (#2093, extracted #2189).
 *
 * Runs two SPARQL queries:
 *   1. products → subproducts → subdomains (nested, with owner)
 *   2. products → direct domains (Borg, Gathering — products that own domains without
 *      an intermediate subproduct)
 * Merges into a flat list of products, each with a `subproducts` array and a
 * `domains` array for direct product→domain edges.
 */

export interface SparqlBinding {
  [key: string]: { value: string } | undefined;
}

export interface SparqlResult {
  results: { bindings: SparqlBinding[] };
}

export type SparqlFn = (query: string) => Promise<SparqlResult>;

export interface ChorusProductsDeps {
  sparql: SparqlFn;
  now?: () => number;
}

export interface ChorusProductsResult {
  status: number;
  body:
    | { products: Array<{ label: string; subproducts: Array<{ label: string; owner: string | null; domains: string[] }>; domains: string[] }>; elapsed_ms: number }
    | { error: string };
}

const NESTED_QUERY = `
  PREFIX chorus: <https://jeffbridwell.com/chorus#>
  PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
  SELECT ?product ?productLabel ?subprod ?spLabel ?subdomain ?sdLabel ?owner ?ownerLabel WHERE {
    GRAPH <urn:chorus:ontology> {
      ?product a chorus:Product . ?product rdfs:label ?productLabel .
      OPTIONAL {
        ?product chorus:hasSubProduct ?subprod . ?subprod rdfs:label ?spLabel .
        OPTIONAL { ?subprod chorus:hasDomain ?subdomain . ?subdomain rdfs:label ?sdLabel }
        OPTIONAL { ?subprod chorus:ownedBy ?owner . ?owner rdfs:label ?ownerLabel }
      }
      OPTIONAL {
        ?product chorus:hasDomain ?subdomain . ?subdomain rdfs:label ?sdLabel .
        FILTER NOT EXISTS { ?subprod2 chorus:hasDomain ?subdomain }
      }
    }
  } ORDER BY ?productLabel ?spLabel ?sdLabel`;

const DIRECT_QUERY = `
  PREFIX chorus: <https://jeffbridwell.com/chorus#>
  PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
  SELECT ?product ?productLabel ?domain ?domainLabel WHERE {
    GRAPH <urn:chorus:ontology> {
      ?product a chorus:Product . ?product rdfs:label ?productLabel .
      ?product chorus:hasDomain ?domain . ?domain rdfs:label ?domainLabel .
    }
  } ORDER BY ?productLabel ?domainLabel`;

interface SubProduct {
  label: string;
  owner: string | null;
  domains: string[];
}

interface ProductAccum {
  label: string;
  subproducts: Partial<Record<string, SubProduct>>;
  domains: string[];
}

type ProductMap = Partial<Record<string, ProductAccum>>;

function ensureProduct(products: ProductMap, pLabel: string): ProductAccum {
  let p = products[pLabel];
  if (!p) { p = { label: pLabel, subproducts: {}, domains: [] }; products[pLabel] = p; }
  return p;
}

function mergeNestedBinding(products: ProductMap, b: Record<string, { value: string } | undefined>): void {
  const p = ensureProduct(products, b.productLabel?.value || '?');
  const spLabel = b.spLabel?.value;
  if (!spLabel) return;
  let sp = p.subproducts[spLabel];
  if (!sp) {
    sp = { label: spLabel, owner: b.ownerLabel?.value || null, domains: [] };
    p.subproducts[spLabel] = sp;
  }
  const sd = b.sdLabel?.value;
  if (sd && !sp.domains.includes(sd)) {
    sp.domains.push(sd);
  }
}

function mergeDirectBinding(products: ProductMap, b: Record<string, { value: string } | undefined>): void {
  const p = ensureProduct(products, b.productLabel?.value || '?');
  const dom = b.domainLabel?.value;
  if (dom && !p.domains.includes(dom)) p.domains.push(dom);
}

export async function fetchChorusProducts({
  sparql,
  now = Date.now,
}: ChorusProductsDeps): Promise<ChorusProductsResult> {
  const start = now();
  try {
    const [nested, direct] = await Promise.all([sparql(NESTED_QUERY), sparql(DIRECT_QUERY)]);
    const products: ProductMap = {};
    nested.results.bindings.forEach((b) => mergeNestedBinding(products, b));
    direct.results.bindings.forEach((b) => mergeDirectBinding(products, b));
    const out = Object.values(products).filter((p): p is ProductAccum => p !== undefined).map((p) => ({
      label: p.label,
      subproducts: Object.values(p.subproducts).filter((sp): sp is SubProduct => sp !== undefined),
      domains: p.domains,
    }));
    return { status: 200, body: { products: out, elapsed_ms: now() - start } };
  } catch (err) {
    return { status: 500, body: { error: err instanceof Error ? err.message : String(err) } };
  }
}
