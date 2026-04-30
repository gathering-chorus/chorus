/**
 * #2485 Move 8 — discover-endpoints scanner for chorus-api's own routes.
 *
 * Sibling to parseAppRoutes (which scans gathering-app's src/app.ts). Tags
 * routes by URL pattern:
 *   /api/loom/<slug>(/.*)? → subdomain `loom-<slug>` (when in valid set)
 *   anything else (concrete) → `chorus-domain` (operational substrate)
 *   parameterized routes with :id / :name → skipped (would need per-subdomain
 *     instantiation; deferred until the contract model supports applies-to-all).
 *
 * Pulled into its own module for unit-testability; mirrors discover-pages-loom.
 */

export interface ChorusApiEndpointEntry {
  method: string;
  path: string;
  handler: string;
  domainId: string;
}

// #2627: route → domainId mapping extracted; loop becomes flat.
function routeToDomainId(routePath: string, validSubdomainIds: Set<string>): string | null {
  const loomMatch = routePath.match(/^\/api\/loom\/([a-z0-9-]+)/);
  if (loomMatch) {
    const candidate = `loom-${loomMatch[1]}`;
    if (validSubdomainIds.has(candidate)) return candidate;
  }
  if (validSubdomainIds.has('chorus-domain')) return 'chorus-domain';
  return null;
}

export function parseChorusApiRoutes(
  appContent: string,
  validSubdomainIds: Set<string>,
): ChorusApiEndpointEntry[] {
  const entries: ChorusApiEndpointEntry[] = [];
  const routeRegex = /app\.(get|post|put|delete|patch)\s*\(\s*['"`]([^'"`]+)['"`]/gi;
  let match;
  while ((match = routeRegex.exec(appContent)) !== null) {
    const method = match[1].toUpperCase();
    const routePath = match[2];
    if (routePath.includes(':')) continue;
    const domainId = routeToDomainId(routePath, validSubdomainIds);
    if (!domainId) continue;
    entries.push({
      method,
      path: routePath,
      handler: 'chorus/platform/api/src/server.ts',
      domainId,
    });
  }
  return entries;
}
