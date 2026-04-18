/**
 * GET /api/athena/subdomains/:id/code — Code files attached to sub-domain (#2187).
 *
 * SPARQL returns files via chorus:hasCodeFile. Handler classifies each
 * file as test or source by path pattern, groups counts by type.
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlCodeBinding {
  file: { value: string };
  label?: { value: string };
  filePath?: { value: string };
  fileType?: { value: string };
  description?: { value: string };
}

export interface SparqlCodeResult {
  results: { bindings: SparqlCodeBinding[] };
}

export interface AthenaSubdomainCodeDeps {
  sparql: (query: string) => Promise<SparqlCodeResult>;
  /** path.extname equivalent — returns extension including leading dot, or empty string */
  extname: (p: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

interface CodeFile {
  path: string;
  type: string;
  description: string | null;
}

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackId(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

function isTest(p: string): boolean {
  return /\/(tests?|__tests__)\//i.test(p)
    || /\.(test|spec)\./i.test(p)
    || /\.bats$/i.test(p)
    || /_test\.rs$/i.test(p)
    || /\.feature$/i.test(p);
}

export async function fetchAthenaSubdomainCode(
  deps: AthenaSubdomainCodeDeps,
  id: string,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  const sdUri = `${CHORUS_PREFIX}${id}`;

  try {
    const query = `PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?file ?label ?filePath ?fileType ?description WHERE { GRAPH <urn:chorus:instances> { <${sdUri}> chorus:hasCodeFile ?file . OPTIONAL { ?file rdfs:label ?label } OPTIONAL { ?file chorus:filePath ?filePath } OPTIONAL { ?file chorus:fileType ?fileType } OPTIONAL { ?file rdfs:comment ?description } } }`;
    const result = await deps.sparql(query);
    const allFiles: CodeFile[] = result.results.bindings.map((b) => {
      const filePath = b.filePath?.value;
      const extType = filePath ? deps.extname(filePath).replace(/^\./, '') : '';
      return {
        path: filePath ?? b.label?.value ?? fallbackId(b.file.value),
        type: b.fileType?.value ?? (extType !== '' ? extType : 'unknown'),
        description: b.description?.value ?? null,
      };
    });
    const tests = allFiles.filter((f) => isTest(f.path));
    const source = allFiles.filter((f) => !isTest(f.path));
    const byType = allFiles.reduce<Record<string, number>>((acc, f) => {
      acc[f.type] = (acc[f.type] ?? 0) + 1;
      return acc;
    }, {});
    return {
      status: 200,
      body: envelope(
        'subdomain-code',
        { subdomain: id, files: source, tests, byType },
        now() - start,
        { count: allFiles.length, source_count: source.length, test_count: tests.length },
      ),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('subdomain-code', { error: message }, now() - start, { error: true }),
    };
  }
}
