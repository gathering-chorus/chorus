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
  subproducts: Record<string, SubProduct>;
  domains: string[];
}

// eslint-disable-next-line complexity -- #2288 pre-existing threshold violation, tracked for refactor
export async function fetchChorusProducts({
  sparql,
  now = Date.now,
}: ChorusProductsDeps): Promise<ChorusProductsResult> {
  const start = now();
  try {
    const result = await sparql(NESTED_QUERY);
    const directResult = await sparql(DIRECT_QUERY);

    const products: Record<string, ProductAccum> = {};

    for (const b of result.results.bindings) {
      const pLabel = b.productLabel?.value || '?';
      if (!products[pLabel]) products[pLabel] = { label: pLabel, subproducts: {}, domains: [] };
      const p = products[pLabel];
      if (b.spLabel?.value) {
        const spLabel = b.spLabel.value;
        if (!p.subproducts[spLabel]) {
          p.subproducts[spLabel] = {
            label: spLabel,
            owner: b.ownerLabel?.value || null,
            domains: [],
          };
        }
        if (b.sdLabel?.value && !p.subproducts[spLabel].domains.includes(b.sdLabel.value)) {
          p.subproducts[spLabel].domains.push(b.sdLabel.value);
        }
      }
    }

    for (const b of directResult.results.bindings) {
      const pLabel = b.productLabel?.value || '?';
      if (!products[pLabel]) products[pLabel] = { label: pLabel, subproducts: {}, domains: [] };
      const domLabel = b.domainLabel?.value;
      if (domLabel && !products[pLabel].domains.includes(domLabel)) {
        products[pLabel].domains.push(domLabel);
      }
    }

    const out = Object.values(products).map((p) => ({
      label: p.label,
      subproducts: Object.values(p.subproducts),
      domains: p.domains,
    }));

    return { status: 200, body: { products: out, elapsed_ms: now() - start } };
  } catch (err) {
    return { status: 500, body: { error: err instanceof Error ? err.message : String(err) } };
  }
}
