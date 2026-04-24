// /api/athena/discover-code logic (extracted from server.ts for #2205 wave 25).

/** SPARQL result bag — caller downcasts the bindings to the expected shape. */
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
  extname: (p: string) => string;
}

interface FsModule {
  existsSync: (p: string) => boolean;
  readdirSync: (p: string, opts?: { withFileTypes?: boolean; encoding?: string; recursive?: boolean }) => string[];
  statSync: (p: string) => { isFile: () => boolean };
}

export interface DiscoverCodeDeps {
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

const OVERRIDES: Record<string, string[]> = {
  'blog-domain': ['blog', 'wordpress'],
  'social-domain': ['social', 'socialpost'],
  'people-domain': ['people', 'person'],
  'documents-domain': ['documents', 'document', 'doc-catalog'],
  'knowledge-domain': ['knowledge', 'knowledge-graph'],
  'sexuality-domain': ['sexuality', 'self-ai'],
  'seeds-domain': ['seeds', 'seed', 'sms-seed'],
  'convergence-domain': ['convergence', 'ontology'],
  'chorus-domain': ['chorus', 'clearing', 'bridge', 'context-cache'],
  'infra-service': ['infrastructure', 'infra', 'app-state', 'agent-state'],
  'observability-service': ['observability', 'dashboard'],
  'cards-service': ['cards', 'board'],
  'skills-service': ['skills'],
  'gates-service': ['gates', 'gate'],
  'spine-service': ['spine', 'chorus-log'],
  'logs-service': ['logs', 'log-freshness'],
  'alerts-service': ['alerts', 'alert'],
  'deploys-service': ['deploys', 'deploy', 'app-state'],
};

export function buildCodeAliasMap(
  domains: Array<{ id?: string; label?: string; sd?: { value?: string } }>,
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const d of domains) {
    const id: string = d.id || String(d.sd?.value || '').split('#').pop() || '';
    if (!id) continue;
    const base = id.replace(/-(domain|service)$/, '');
    if (GENERIC_BASES.has(base)) continue;
    const aliases = [base];
    if (base.endsWith('s') && !base.endsWith('ss')) {
      if (base.endsWith('ies')) aliases.push(base.replace(/ies$/, 'y'));
      else aliases.push(base.replace(/s$/, ''));
    }
    out[id] = aliases;
  }
  for (const [k, v] of Object.entries(OVERRIDES)) out[k] = v;
  return out;
}

type Discovered = { domainId: string; filePath: string; fileType: string };

function classifyEntry(_entryStr: string, basename: string, pathParts: string[], ext: string, aliasMap: Record<string, string[]>, qualifiedPath: string): Discovered | null {
  for (const [domainId, aliases] of Object.entries(aliasMap)) {
    for (const alias of aliases) {
      const nameMatch = basename.includes(alias) || basename.startsWith(alias + '.') || basename.startsWith(alias + '-');
      const pathMatch = pathParts.some((part) => part === alias || part === alias + 's');
      if (nameMatch || pathMatch) {
        return { domainId, filePath: qualifiedPath, fileType: ext };
      }
    }
  }
  return null;
}

function makeScanDir(deps: DiscoverCodeDeps, aliasMap: Record<string, string[]>, discovered: Discovered[]) {
  const repoName = (repoRoot: string) => repoRoot === deps.gatheringRoot ? 'gathering' : 'chorus';
  return (dir: string, repoRoot: string) => {
    if (!deps.fs.existsSync(dir)) return;
    const entries = deps.fs.readdirSync(dir, { recursive: true }) as string[];
    for (const entry of entries) {
      const entryStr = String(entry);
      if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
      const fullPath = deps.path.join(dir, entryStr);
      try { if (!deps.fs.statSync(fullPath).isFile()) continue; } catch { continue; }
      const relPath = deps.path.relative(repoRoot, fullPath);
      const qualifiedPath = `${repoName(repoRoot)}/${relPath}`;
      const basename = deps.path.basename(entryStr).toLowerCase();
      const pathParts = relPath.toLowerCase().split('/');
      const ext = deps.path.extname(entryStr).slice(1) || 'unknown';
      const hit = classifyEntry(entryStr, basename, pathParts, ext, aliasMap, qualifiedPath);
      if (hit) discovered.push(hit);
    }
  };
}

function scanOverrideDir(deps: DiscoverCodeDeps, dir: string, domainId: string, discovered: Discovered[]): void {
  const fullDir = deps.path.join(deps.chorusRoot, dir);
  if (!deps.fs.existsSync(fullDir)) return;
  const entries = deps.fs.readdirSync(fullDir, { recursive: true }) as string[];
  for (const entry of entries) {
    const entryStr = String(entry);
    if (entryStr.includes('node_modules') || entryStr.includes('.git') || entryStr.includes('dist/')) continue;
    const fullPath = deps.path.join(fullDir, entryStr);
    try { if (!deps.fs.statSync(fullPath).isFile()) continue; } catch { continue; }
    const relPath = deps.path.relative(deps.chorusRoot, fullPath);
    const ext = deps.path.extname(entryStr).slice(1) || 'unknown';
    discovered.push({ domainId, filePath: `chorus/${relPath}`, fileType: ext });
  }
}

async function writeDiscoveredInBatches(deps: DiscoverCodeDeps, discovered: Discovered[]): Promise<number> {
  const batchSize = 50;
  let written = 0;
  for (let i = 0; i < discovered.length; i += batchSize) {
    const batch = discovered.slice(i, i + batchSize);
    const triples = batch.map((d) => {
      const fileId = `${d.domainId}-code-${d.filePath.replace(/[/.]/g, '-').toLowerCase()}`;
      const fileUri = `https://jeffbridwell.com/chorus#${fileId}`;
      const sdUri = `https://jeffbridwell.com/chorus#${d.domainId}`;
      return `<${fileUri}> a chorus:CodeFile ; rdfs:label "${d.filePath.replace(/"/g, '\\"')}" ; chorus:filePath "${d.filePath.replace(/"/g, '\\"')}" ; chorus:fileType "${d.fileType}" . <${sdUri}> chorus:hasCodeFile <${fileUri}> .`;
    }).join('\n');
    const insert = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} } }`;
    await deps.sparqlClient.update(insert);
    written += batch.length;
  }
  return written;
}

export function createDiscoverCode(deps: DiscoverCodeDeps) {
  return async function discoverCode() {
    const sdQuery = 'PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?sd ?label WHERE { GRAPH <urn:chorus:ontology> { ?sd a chorus:SubDomain ; rdfs:label ?label } }';
    const sdResult = await deps.sparqlClient.query(sdQuery);
    const bindings = sdResult.results?.bindings ?? [];
    const domains = bindings.map((b) => ({
      id: b.sd.value.split('#').pop() as string,
      label: b.label.value,
    }));
    const aliasMap = buildCodeAliasMap(domains);
    const discovered: Discovered[] = [];
    const scanDir = makeScanDir(deps, aliasMap, discovered);

    scanDir(deps.path.join(deps.gatheringRoot, 'src/handlers'), deps.gatheringRoot);
    scanDir(deps.path.join(deps.gatheringRoot, 'src/services'), deps.gatheringRoot);
    scanDir(deps.path.join(deps.gatheringRoot, 'src/adapters'), deps.gatheringRoot);
    scanDir(deps.path.join(deps.gatheringRoot, 'tests'), deps.gatheringRoot);
    scanDir(deps.path.join(deps.chorusRoot, 'platform/scripts'), deps.chorusRoot);
    scanDir(deps.path.join(deps.chorusRoot, 'platform/services/chorus-hooks/src'), deps.chorusRoot);

    const dirDomainOverrides: Record<string, string> = {
      'platform/api/src': 'chorus-domain',
      'platform/api/tests': 'chorus-domain',
    };
    for (const [dir, domainId] of Object.entries(dirDomainOverrides)) {
      scanOverrideDir(deps, dir, domainId, discovered);
    }
    scanDir(deps.path.join(deps.chorusRoot, 'skills'), deps.chorusRoot);
    scanDir(deps.path.join(deps.chorusRoot, 'proving/domains/alerts'), deps.chorusRoot);

    const clearQuery = 'DELETE WHERE { GRAPH <urn:chorus:instances> { ?file a <https://jeffbridwell.com/chorus#CodeFile> ; ?p ?o . ?sd <https://jeffbridwell.com/chorus#hasCodeFile> ?file . } }';
    await deps.sparqlClient.update(clearQuery);

    const written = await writeDiscoveredInBatches(deps, discovered);

    const byDomain: Record<string, number> = {};
    for (const d of discovered) byDomain[d.domainId] = (byDomain[d.domainId] || 0) + 1;

    return {
      total_files: discovered.length,
      total_domains: Object.keys(byDomain).length,
      domains_available: domains.length,
      by_domain: byDomain,
      written,
    };
  };
}
