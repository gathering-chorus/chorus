/* eslint-disable security/detect-non-literal-fs-filename -- internal scanner: paths from the repo public/loom dir, never untrusted input (#3429) */
/**
 * #2485 Move 6 — scanner for chorus/platform/api/public/loom/*.html.
 *
 * Each <slug>.html maps to subdomain id `loom-<slug>` if that subdomain
 * exists in the graph. #3656 adds directory-style pages (<slug>/index.html →
 * route /loom/<slug>/), the express-static shape the borg surfaces use.
 * Sibling to scanEjsViews / scanDocHtml in server.ts; pulled into its own
 * module for unit-testability.
 */
import * as fs from 'node:fs';

export interface LoomPageEntry {
  route: string;
  path: string;
  pageType: string;
  domainId: string;
}

// #3656 — placement overrides for pages whose slug is not a subdomain.
// quality → loom-practices: Silas's stamp — ADR-026 quality layers are team
// practice; no single-page loom-quality subdomain gets spawned. Mirrors
// DISCOVER_PAGES_ALIAS_OVERRIDES in server.ts.
const LOOM_SLUG_OVERRIDES: Record<string, string> = {
  quality: 'loom-practices',
};

export function scanLoomHtml(loomDir: string, validSubdomainIds: Set<string>): LoomPageEntry[] {
  const entries: LoomPageEntry[] = [];
  if (!fs.existsSync(loomDir)) return entries;
  for (const dirent of fs.readdirSync(loomDir, { withFileTypes: true })) {
    let slug: string;
    let route: string;
    let relPath: string;
    if (dirent.isFile() && dirent.name.endsWith('.html')) {
      slug = dirent.name.replace(/\.html$/, '');
      route = `/loom/${dirent.name}`;
      relPath = `chorus/platform/api/public/loom/${dirent.name}`;
    } else if (dirent.isDirectory() && fs.existsSync(`${loomDir}/${dirent.name}/index.html`)) {
      slug = dirent.name;
      route = `/loom/${dirent.name}/`;
      relPath = `chorus/platform/api/public/loom/${dirent.name}/index.html`;
    } else {
      continue;
    }
    const domainId = LOOM_SLUG_OVERRIDES[slug] ?? `loom-${slug}`;
    if (!validSubdomainIds.has(domainId)) continue;
    entries.push({ route, path: relPath, pageType: 'loom', domainId });
  }
  return entries;
}
