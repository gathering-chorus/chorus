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
  readdirSync: (p: string, opts?: { withFileTypes?: boolean; encoding?: string; recursive?: boolean }) => string[];
  statSync: (p: string) => { isFile: () => boolean };
}

export interface DiscoverTestsDeps {
  sparqlClient: SparqlClient;
  fs: FsModule;
  path: PathModule;
  gatheringRoot: string;
  chorusRoot: string;
}

const GENERIC_BASES = new Set([
  'services', 'service', 'domains', 'domain', 'code', 'loom', 'time',
  'streams', 'stream', 'messages', 'message', 'policies', 'policy',
]);

const SPECIAL_ALIASES: Record<string, string> = {
  wordpress: 'blog-domain',
  socialpost: 'social-domain',
  'sms-seed': 'seeds-domain',
  'self-ai': 'sexuality-domain',
  ontology: 'convergence-domain',
};

export function classifyTestType(relPath: string): string {
  if (/\/e2e\//i.test(relPath) || /\.e2e\./i.test(relPath)) return 'e2e';
  if (/\/integration\//i.test(relPath)) return 'integration';
  if (/\/performance\//i.test(relPath)) return 'performance';
  if (/\/security\//i.test(relPath)) return 'security';
  if (/\.bats$/i.test(relPath)) return 'bdd';
  if (/\.feature$/i.test(relPath)) return 'bdd';
  return 'unit';
}

export function buildAliasMap(
  domains: Array<{ id?: string; label?: string; sd?: { value?: string } }>,
): Record<string, string> {
  const out: Record<string, string> = {};
  const normalized = domains.map((d) => {
    if (d.id) return { id: d.id, label: d.label ?? '' };
    return {
      id: String(d.sd?.value ?? '').split('#').pop() || '',
      label: String(d.label ?? '').toLowerCase(),
    };
  });
  for (const d of normalized) {
    if (!d.id) continue;
    const base = d.id.replace(/-(domain|service)$/, '');
    if (GENERIC_BASES.has(base)) continue;
    out[base] = d.id;
    if (base.endsWith('s') && !base.endsWith('ss')) {
      if (base.endsWith('ies')) out[base.replace(/ies$/, 'y')] = d.id;
      else out[base.replace(/s$/, '')] = d.id;
    }
  }
  for (const [k, v] of Object.entries(SPECIAL_ALIASES)) out[k] = v;
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
    const sdQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }';
    const sdResult = await deps.sparqlClient.query(sdQuery);
    const bindings = sdResult.results?.bindings ?? [];
    const aliasToId = buildAliasMap(bindings.map((b) => ({ sd: b.sd, label: b.label.value })));

    const testEntries: Array<{ testFile: string; testType: string; coversDomain: string }> = [];

    const scanTests = (dir: string, repoRoot: string) => {
      if (!deps.fs.existsSync(dir)) return;
      const prefix = repoRoot === deps.gatheringRoot ? 'gathering' : 'chorus';
      const entries = deps.fs.readdirSync(dir, { recursive: true }) as string[];
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
