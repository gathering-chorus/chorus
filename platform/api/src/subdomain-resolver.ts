// Subdomain resolver + test-file classifier (extracted from server.ts
// for #2205 wave 10).
//
// resolveSubdomainId maps a domain NAME ("seeds") to the ontology's
// subdomain ID ("seeds-domain" or "seeds-service"). Explicit forms pass
// through unchanged. Unknown forms ASK Fuseki whether -domain exists and
// fall back to -service when the query says no (or fails).

export interface SubdomainResolverDeps {
  sparql: (query: string) => Promise<{ boolean?: boolean }>;
}

export function createSubdomainResolver(deps: SubdomainResolverDeps): (name: string) => Promise<string> {
  return async function resolveSubdomainId(name: string): Promise<string> {
    const lower = name.toLowerCase();
    if (lower.endsWith('-domain') || lower.endsWith('-service')) return lower;
    const domainId = `${lower}-domain`;
    const svcId = `${lower}-service`;
    const checkQuery = `PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <urn:chorus:ontology> { <https://jeffbridwell.com/chorus#${domainId}> a chorus:SubDomain } }`;
    try {
      const result = await deps.sparql(checkQuery);
      if (result.boolean) return domainId;
    } catch {
      /* fall through to -service */
    }
    return svcId;
  };
}

/** Match test-file paths by conventional suffix / directory. */
export function isTestFile(p: string): boolean {
  return /\/(tests?|__tests__)\//i.test(p)
    || /\.(test|spec)\./i.test(p)
    || /\.bats$/i.test(p)
    || /_test\.rs$/i.test(p)
    || /\.feature$/i.test(p);
}
