/**
 * GET /api/chorus/domain/:domain/code-files — flat file list for a domain (#2188).
 *
 * Dependencies injected:
 *   sparql — async (query) => { results: { bindings: Array<{filePath: {value: string}}> } }
 *
 * Behavior:
 *   - Tries <domain>-domain first, then <domain>-service if empty
 *   - Both failing → {files: [], count: 0}
 *   - Domain already ending in -domain or -service → skip fallback
 */
import type { FetchResult } from './codebase-topology';

interface SparqlBinding {
  filePath: { value: string };
}

interface SparqlResult {
  results: { bindings: SparqlBinding[] };
}

export interface ChorusCodeFilesDeps {
  sparql: (query: string) => Promise<SparqlResult>;
}

function query(subjectSuffix: string): string {
  return `PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?filePath WHERE { GRAPH <urn:chorus:instances> { <https://jeffbridwell.com/chorus#${subjectSuffix}> chorus:hasCodeFile ?file . ?file chorus:filePath ?filePath . } }`;
}

export async function fetchChorusCodeFiles(
  deps: ChorusCodeFilesDeps,
  domainParam: string,
): Promise<FetchResult> {
  const domain = domainParam.toLowerCase();
  const files: string[] = [];

  try {
    const domainSuffix = domain.endsWith('-domain') || domain.endsWith('-service') ? domain : `${domain}-domain`;
    const result = await deps.sparql(query(domainSuffix));
    files.push(...result.results.bindings.map((b) => b.filePath.value));

    if (files.length === 0 && !domain.endsWith('-service') && !domain.endsWith('-domain')) {
      const svcResult = await deps.sparql(query(`${domain}-service`));
      files.push(...svcResult.results.bindings.map((b) => b.filePath.value));
    }
  } catch { /* graph query failed */ }

  return {
    status: 200,
    body: { domain, files, count: files.length },
  };
}
