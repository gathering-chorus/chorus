/**
 * GET /api/chorus/domain/:name/code — source files for a domain (#2188).
 *
 * Dependencies injected:
 *   sparql             — async (query) => SparqlResult
 *   resolveSubdomainId — async (name) => string
 *   envelope           — (queryName, data, durationMs, extra) => wrapped body
 *   now                — () => number (default Date.now)
 *
 * Behavior:
 *   - Error path returns 200 envelope with empty files, count 0 (legacy behavior)
 *   - Filters test files from source count via isTestFile regex
 *   - path/type fallback chain: filePath → label → last URI segment
 *   - byType aggregation on source files only
 */
import * as path from 'path';
import type { FetchResult } from './codebase-topology';

interface SparqlBindingValue { value: string }
interface SparqlCodeBinding {
  file: SparqlBindingValue;
  label?: SparqlBindingValue;
  filePath?: SparqlBindingValue;
  fileType?: SparqlBindingValue;
  description?: SparqlBindingValue;
}
interface SparqlCodeResult {
  results: { bindings: SparqlCodeBinding[] };
}

type Sparql = (query: string) => Promise<SparqlCodeResult>;
type ResolveSubdomainId = (name: string) => Promise<string>;
type Envelope = (queryName: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;

export interface ChorusDomainCodeDeps {
  sparql: Sparql;
  resolveSubdomainId: ResolveSubdomainId;
  envelope: Envelope;
  now?: () => number;
}

export function isTestFile(p: string): boolean {
  return /\/(tests?|__tests__)\//i.test(p)
    || /\.(test|spec)\./i.test(p)
    || /\.bats$/i.test(p)
    || /_test\.rs$/i.test(p)
    || /\.feature$/i.test(p);
}

export async function fetchChorusDomainCode(
  deps: ChorusDomainCodeDeps,
  name: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();

  try {
    const sdId = await deps.resolveSubdomainId(name);
    const sdUri = `https://jeffbridwell.com/chorus#${sdId}`;
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?file ?label ?filePath ?fileType ?description WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasCodeFile ?file . OPTIONAL { ?file rdfs:label ?label } OPTIONAL { ?file chorus:filePath ?filePath } OPTIONAL { ?file chorus:fileType ?fileType } OPTIONAL { ?file rdfs:comment ?description } } }`;
    const result = await deps.sparql(query);

    const allFiles = result.results.bindings.map((b) => ({
      path: b.filePath?.value || b.label?.value || b.file.value.split('#').pop() || '',
      type: b.fileType?.value || path.extname(b.filePath?.value || '').slice(1) || 'unknown',
      description: b.description?.value || null,
    }));

    const source = allFiles.filter((f) => !isTestFile(f.path));
    const byType = source.reduce<Record<string, number>>((acc, f) => {
      acc[f.type] = (acc[f.type] || 0) + 1;
      return acc;
    }, {});

    return {
      status: 200,
      body: deps.envelope(
        'domain-code',
        { subdomain: sdId, files: source, byType },
        now() - start,
        { count: allFiles.length, source_count: source.length, test_count: allFiles.length - source.length },
      ),
    };
  } catch {
    return {
      status: 200,
      body: deps.envelope(
        'domain-code',
        { subdomain: name, files: [], byType: {} },
        now() - start,
        { count: 0, source_count: 0, test_count: 0 },
      ),
    };
  }
}
