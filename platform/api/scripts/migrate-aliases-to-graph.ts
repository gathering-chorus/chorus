// #2516 — One-shot migration: write the test→subdomain alias map as triples
// in urn:chorus:ontology, retiring the auto-derivation logic in
// buildAliasMap.
//
// For each SubDomain that does not have a generic-base id, emit triples for
// the singular/plural/compound aliases that buildAliasMap previously derived
// at runtime. Plus the 5 SPECIAL_ALIASES.
//
// Idempotent: deletes existing chorus:hasTestPathPrefix triples in the
// ontology graph, then inserts fresh ones.
//
// Run: npx ts-node platform/api/scripts/migrate-aliases-to-graph.ts

const FUSEKI_QUERY = process.env.FUSEKI_QUERY ?? 'http://localhost:3030/pods/sparql';
const FUSEKI_UPDATE = process.env.FUSEKI_UPDATE ?? 'http://localhost:3030/pods/update';

const GENERIC_BASES = new Set([
  'services', 'service', 'domains', 'domain', 'code', 'loom', 'time',
  'streams', 'stream', 'messages', 'message', 'policies', 'policy',
  'tests', 'test', 'unit', 'integration', 'e2e', 'mocks',
]);

const SPECIAL_ALIASES: Record<string, string> = {
  wordpress: 'blog-domain',
  socialpost: 'social-domain',
  'sms-seed': 'seeds-domain',
  'self-ai': 'sexuality-domain',
  ontology: 'convergence-domain',
};

interface SubDomainRow { id: string; label: string }

export function deriveAliases(rows: SubDomainRow[]): Array<[string, string]> {
  const pairs: Array<[string, string]> = [];
  for (const row of rows) {
    const id = row.id;
    if (!id) continue;
    const base = id.replace(/-(domain|service)$/, '');
    if (GENERIC_BASES.has(base)) continue;
    const aliases = new Set<string>();
    aliases.add(base);
    if (base.endsWith('s') && !base.endsWith('ss')) {
      if (base.endsWith('ies')) aliases.add(base.replace(/ies$/, 'y'));
      else aliases.add(base.replace(/s$/, ''));
    }
    if (base.includes('-')) aliases.add(base);
    for (const a of [...aliases].sort()) pairs.push([a, id]);
  }
  for (const [alias, id] of Object.entries(SPECIAL_ALIASES)) pairs.push([alias, id]);
  return pairs;
}

async function fetchSubDomains(): Promise<SubDomainRow[]> {
  const q = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> '
    + 'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> '
    + 'SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { '
    + '?sd a chorus:SubDomain ; rdfs:label ?label } } ORDER BY ?sd';
  const res = await fetch(FUSEKI_QUERY, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/sparql-query',
      Accept: 'application/sparql-results+json',
    },
    body: q,
  });
  if (!res.ok) throw new Error(`SPARQL query failed: ${res.status}`);
  const data = await res.json() as { results: { bindings: Array<{ sd: { value: string }; label: { value: string } }> } };
  return data.results.bindings.map((b) => ({
    id: b.sd.value.split('#').pop() || '',
    label: b.label.value,
  }));
}

async function runUpdate(update: string): Promise<void> {
  const res = await fetch(FUSEKI_UPDATE, {
    method: 'POST',
    headers: { 'Content-Type': 'application/sparql-update' },
    body: update,
  });
  if (!res.ok) throw new Error(`SPARQL update failed: ${res.status} ${await res.text()}`);
}

async function main(): Promise<void> {
  const rows = await fetchSubDomains();
  const pairs = deriveAliases(rows);

  const escape = (s: string): string => s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  const triples = pairs.map(([alias, id]) =>
    `<https://jeffbridwell.com/chorus#${id}> chorus:hasTestPathPrefix "${escape(alias)}" .`
  ).join('\n');

  const clearQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> '
    + 'DELETE WHERE { GRAPH <urn:chorus:ontology> { ?sd chorus:hasTestPathPrefix ?p } }';
  const insertQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> '
    + `INSERT DATA { GRAPH <urn:chorus:ontology> { ${triples} } }`;

  console.log(`Migration: ${pairs.length} alias triples for ${rows.length} subdomains`);
  await runUpdate(clearQuery);
  await runUpdate(insertQuery);
  console.log('Migration complete.');
}

if (require.main === module) {
  main().catch((e) => { console.error(e); process.exit(1); });
}
