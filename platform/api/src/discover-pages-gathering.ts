/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection -- internal page scanner: paths from the gathering repo root, object keys from a fixed known set, never untrusted input (#3429) */
/**
 * Gathering page scanners — extracted from server.ts (#3097).
 *
 * scanEjsViews / scanDocHtml map gathering's EJS views + gathering-docs HTML to
 * chorus subdomain ids. Sibling to discover-pages-loom.ts; pulled into its own
 * module so the gathering-absent soft-fail (#3097 blocker 1) is unit-testable
 * without importing server.ts (and booting its sqlite/lance handles).
 *
 * Soft-fail contract: an absent root is a SKIPPED scan (empty result), never a
 * thrown error — chorus-api must survive gathering being moved or removed.
 */
import * as fs from 'fs';
import * as path from 'path';

export type PageEntry = { route: string; path: string; pageType: string; domainId: string };

// #2627: ejs-view classifier table — first matching prefix-pattern wins.
type EjsRule = { re: RegExp; build: (m: RegExpMatchArray, name: string, domainId: string) => PageEntry };
const EJS_RULES: EjsRule[] = [
  { re: /^collection-(.+?)(-list)?$/, build: (m, _n, d) => ({ route: `/${m[1]}`, path: '', pageType: 'collection', domainId: d }) },
  { re: /^(.+?)-(detail|album|artist|artists|create)$/, build: (m, _n, d) => ({ route: `/${m[1]}/:slug`, path: '', pageType: 'detail', domainId: d }) },
  { re: /^admin-(?:harvest-)?(.+?)(?:-add)?$/, build: (_m, n, d) => ({ route: `/admin/${n.replace('admin-', '')}`, path: '', pageType: 'admin', domainId: d }) },
];

function classifyEjsView(name: string, aliasToId: Record<string, string>): PageEntry | null {
  for (const rule of EJS_RULES) {
    const m = name.match(rule.re);
    const domainId = m ? aliasToId[m[1]] : undefined;
    if (m && domainId) return rule.build(m, name, domainId);
  }
  const direct = aliasToId[name];
  if (direct) return { route: `/${name}`, path: '', pageType: 'page', domainId: direct };
  for (const [alias, did] of Object.entries(aliasToId)) {
    if (name.startsWith(alias + '-') || name === alias) {
      return { route: `/${name}`, path: '', pageType: 'page', domainId: did };
    }
  }
  return null;
}

export function scanEjsViews(viewsDir: string, aliasToId: Record<string, string>): PageEntry[] {
  const entries: PageEntry[] = [];
  if (!fs.existsSync(viewsDir)) return entries;
  for (const file of fs.readdirSync(viewsDir).filter((f) => f.endsWith('.ejs'))) {
    const classified = classifyEjsView(file.replace('.ejs', ''), aliasToId);
    if (classified) entries.push({ ...classified, path: `gathering/views/${file}` });
  }
  const ontologyDir = path.join(viewsDir, 'ontology-views');
  if (fs.existsSync(ontologyDir)) {
    for (const file of fs.readdirSync(ontologyDir).filter((f) => f.endsWith('.ejs'))) {
      const name = file.replace('.ejs', '');
      const domainId = aliasToId[name];
      if (domainId) {
        entries.push({ route: `/ontology-views/${name}`, path: `gathering/views/ontology-views/${file}`, pageType: 'ontology', domainId });
      }
    }
  }
  return entries;
}

function classifyDocHtml(name: string, aliasToId: Record<string, string>): { domainId: string; pageType: string } | null {
  const domainMatch = name.match(/^domain-(.+)$/);
  if (domainMatch && aliasToId[domainMatch[1]]) return { domainId: aliasToId[domainMatch[1]], pageType: 'doc' };
  const serviceMatch = name.match(/^(.+?)-service-design$/);
  if (serviceMatch && aliasToId[serviceMatch[1]]) return { domainId: aliasToId[serviceMatch[1]], pageType: 'service-design' };
  return null;
}

export function scanDocHtml(docsDir: string, aliasToId: Record<string, string>): PageEntry[] {
  const entries: PageEntry[] = [];
  if (!fs.existsSync(docsDir)) return entries;
  for (const file of fs.readdirSync(docsDir).filter((f) => f.endsWith('.html'))) {
    const classified = classifyDocHtml(file.replace('.html', ''), aliasToId);
    if (classified) {
      entries.push({
        route: `/gathering-docs/${file}`,
        path: `gathering/public/gathering-docs/${file}`,
        pageType: classified.pageType,
        domainId: classified.domainId,
      });
    }
  }
  return entries;
}
