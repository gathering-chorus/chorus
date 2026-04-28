/* eslint-disable security/detect-object-injection --
 * Indexing on SPARQL binding keys with fixed schema.
 */
// /api/athena/discover-tests logic (extracted from server.ts for #2205 wave 24).
// Returns a flat summary object; server.ts wraps it in athenaEnvelope.

interface SparqlResult { results?: { bindings?: Array<Record<string, { value: string }>> } }

interface SparqlClient {
  query: (q: string) => Promise<SparqlResult>;
  update: (u: string) => Promise<void>;
}

interface PathModule {
  join: (...parts: string[]) => string;
  resolve: (...parts: string[]) => string;
  relative: (from: string, to: string) => string;
  basename: (p: string) => string;
}

interface FsModule {
  existsSync: (p: string) => boolean;
  readdirSync: (p: string, opts?: BufferEncoding | { encoding: BufferEncoding | null; withFileTypes?: false; recursive?: boolean } | null) => string[];
  statSync: (p: string) => { isFile: () => boolean };
}

export interface DiscoverTestsDeps {
  sparqlClient: SparqlClient;
  fs: FsModule;
  path: PathModule;
  gatheringRoot: string;
  chorusRoot: string;
}

// #2516: GENERIC_BASES + SPECIAL_ALIASES + buildAliasMap auto-derivation
// retired. Aliases are graph-resident as <sd> chorus:hasTestPathPrefix
// "alias" triples in urn:chorus:ontology. Migration: scripts/migrate-aliases-to-graph.ts.
// New subdomains declare their own hasTestPathPrefix triples at creation time.

export function classifyTestType(relPath: string): string {
  if (/\/e2e\//i.test(relPath) || /\.e2e\./i.test(relPath)) return 'e2e';
  if (/\/integration\//i.test(relPath)) return 'integration';
  if (/\/performance\//i.test(relPath)) return 'performance';
  if (/\/security\//i.test(relPath)) return 'security';
  if (/\.bats$/i.test(relPath)) return 'bdd';
  if (/\.feature$/i.test(relPath)) return 'bdd';
  return 'unit';
}

export function loadAliasMap(
  rows: Array<{ sd?: { value?: string }; prefix?: { value?: string } }>,
): Record<string, string> {
  // Reads alias triples from urn:chorus:ontology and produces the runtime
  // aliasToId map. Order matters: when two SubDomains claim the same
  // prefix (e.g. property), the SPARQL query's ORDER BY ?sd determines
  // which wins under last-write-wins dict semantics.
  const out: Record<string, string> = {};
  for (const r of rows) {
    const id = String(r.sd?.value ?? '').split('#').pop() || '';
    const prefix = String(r.prefix?.value ?? '');
    if (!id || !prefix) continue;
    out[prefix] = id;
  }
  return out;
}

export function inferDomain(
  filePath: string,
  aliasToId: Record<string, string>,
  path: PathModule,
): string | null {
  const basename = path.basename(filePath).toLowerCase();
  const pathLower = filePath.toLowerCase();
  for (const [alias, domainId] of Object.entries(aliasToId)) {
    if (basename.includes(alias) || pathLower.split('/').some(p => p === alias || p === alias + 's')) {
      return domainId;
    }
  }
  return null;
}

export function createDiscoverTests(deps: DiscoverTestsDeps) {
  return async function discoverTests() {
    const aliasQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> '
      + 'SELECT ?sd ?prefix WHERE { GRAPH <urn:chorus:ontology> { '
      + '?sd chorus:hasTestPathPrefix ?prefix } } ORDER BY ?sd';
    const aliasResult = await deps.sparqlClient.query(aliasQuery);
    const bindings = aliasResult.results?.bindings ?? [];
    const aliasToId = loadAliasMap(bindings.map((b) => ({ sd: b.sd, prefix: b.prefix })));

    const testEntries: Array<{ testFile: string; testType: string; coversDomain: string }> = [];

    const scanTests = (dir: string, repoRoot: string) => {
      if (!deps.fs.existsSync(dir)) return;
      const prefix = repoRoot === deps.gatheringRoot ? 'gathering' : 'chorus';
      const entries = deps.fs.readdirSync(dir, { recursive: true, encoding: null }) as string[];
      for (const entry of entries) {
        const entryStr = String(entry);
        if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
        const fullPath = deps.path.join(dir, entryStr);
        try { if (!deps.fs.statSync(fullPath).isFile()) continue; } catch { continue; }
        if (!/\.(test|spec)\.(ts|js)$|\.bats$|\.feature$/i.test(entryStr)) continue;
        const relPath = deps.path.relative(repoRoot, fullPath);
        const qualifiedPath = `${prefix}/${relPath}`;
        const testType = classifyTestType(relPath);
        const coversDomain = inferDomain(relPath, aliasToId, deps.path);
        if (coversDomain) testEntries.push({ testFile: qualifiedPath, testType, coversDomain });
      }
    };

    scanTests(deps.path.join(deps.gatheringRoot, 'tests'), deps.gatheringRoot);
    scanTests(deps.path.join(deps.chorusRoot, 'platform/api/tests'), deps.chorusRoot);
    scanTests(deps.path.join(deps.chorusRoot, 'platform/services/chorus-hooks/tests'), deps.chorusRoot);
    scanTests(deps.path.join(deps.chorusRoot, 'proving'), deps.chorusRoot);
    scanTests(deps.path.join(deps.chorusRoot, 'docs/diagrams'), deps.chorusRoot);
    // #2515 — extend crawl to cover the cards CLI test suite (47 files) and
    // the bats suite under platform/tests (knowledge/seeds/auto-role-state etc).
    scanTests(deps.path.join(deps.chorusRoot, 'directing/products/cards/tests'), deps.chorusRoot);
    scanTests(deps.path.join(deps.chorusRoot, 'platform/tests'), deps.chorusRoot);

    const clearQuery = 'DELETE WHERE { GRAPH <urn:chorus:instances> { ?t a <https://jeffbridwell.com/chorus#TestCoverage> ; ?p ?o . } }';
    await deps.sparqlClient.update(clearQuery);

    const batchSize = 50;
    let written = 0;
    for (let i = 0; i < testEntries.length; i += batchSize) {
      const batch = testEntries.slice(i, i + batchSize);
      const triples = batch.map(t => {
        const tcId = `test-coverage-${t.testFile.replace(/[/.]/g, '-').toLowerCase()}`;
        const tcUri = `https://jeffbridwell.com/chorus#${tcId}`;
        const sdUri = `https://jeffbridwell.com/chorus#${t.coversDomain}`;
        return `<${tcUri}> a chorus:TestCoverage ; chorus:testFile "${t.testFile.replace(/"/g, '\\"')}" ; chorus:testType "${t.testType}" ; chorus:covers <${sdUri}> .`;
      }).join('\n');
      const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
      await deps.sparqlClient.update(insert);
      written += batch.length;
    }

    const byType: Record<string, number> = {};
    const byDomain: Record<string, number> = {};
    for (const t of testEntries) {
      byType[t.testType] = (byType[t.testType] || 0) + 1;
      byDomain[t.coversDomain] = (byDomain[t.coversDomain] || 0) + 1;
    }

    return {
      total_tests: testEntries.length,
      total_domains_covered: Object.keys(byDomain).length,
      by_type: byType,
      by_domain: byDomain,
      written,
    };
  };
}
