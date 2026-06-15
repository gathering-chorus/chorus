/* eslint-disable security/detect-non-literal-fs-filename -- internal scanner: paths from the repo public/athena dir, never untrusted input (#3429) */
/**
 * #2041 — scanner for chorus/platform/api/public/athena/*.html.
 *
 * Athena UI relocated out of jeff-bridwell-personal-site. The HTML files
 * served at /athena/* belong to athena-domain. This mirrors discover-pages-loom.ts.
 */
import * as fs from 'node:fs';

export interface AthenaPageEntry {
  route: string;
  path: string;
  pageType: string;
  domainId: string;
}

export function scanAthenaHtml(athenaDir: string, validSubdomainIds: Set<string>): AthenaPageEntry[] {
  const entries: AthenaPageEntry[] = [];
  if (!fs.existsSync(athenaDir)) return entries;
  if (!validSubdomainIds.has('athena-domain')) return entries;
  for (const file of fs.readdirSync(athenaDir).filter((f) => f.endsWith('.html'))) {
    entries.push({
      route: `/athena/${file}`,
      path: `chorus/platform/api/public/athena/${file}`,
      pageType: 'athena',
      domainId: 'athena-domain',
    });
  }
  return entries;
}
