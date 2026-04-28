/**
 * #2485 Move 6 — scanner for chorus/platform/api/public/loom/*.html.
 *
 * Each <slug>.html maps to subdomain id `loom-<slug>` if that subdomain
 * exists in the graph. Sibling to scanEjsViews / scanDocHtml in server.ts;
 * pulled into its own module for unit-testability.
 */
import * as fs from 'node:fs';

export interface LoomPageEntry {
  route: string;
  path: string;
  pageType: string;
  domainId: string;
}

export function scanLoomHtml(loomDir: string, validSubdomainIds: Set<string>): LoomPageEntry[] {
  const entries: LoomPageEntry[] = [];
  if (!fs.existsSync(loomDir)) return entries;
  for (const file of fs.readdirSync(loomDir).filter((f) => f.endsWith('.html'))) {
    const slug = file.replace(/\.html$/, '');
    const domainId = `loom-${slug}`;
    if (!validSubdomainIds.has(domainId)) continue;
    entries.push({
      route: `/loom/${file}`,
      path: `chorus/platform/api/public/loom/${file}`,
      pageType: 'loom',
      domainId,
    });
  }
  return entries;
}
