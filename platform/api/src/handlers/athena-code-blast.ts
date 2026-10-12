// @domain: code
/**
 * #4499 — "what breaks if I change X", from the code units the crawler writes
 * on every land (CodeUnit + dependsOn, Service builtFrom). Jeff, 2026-10-11
 * 10:00: the old answer listed every file in a domain (clearing: 1,748) and
 * could not find a service. Wren, 16:41: answer in services, domains, tests and
 * pages — never a file list — the same for a file and for a service, direct
 * first, then through dependencies, with counts.
 */
const C = 'https://jeffbridwell.com/chorus#';
const local = (iri: string): string => (iri.startsWith(C) ? iri.slice(C.length) : iri);

export interface UnitRow { name: string; path: string; kind: string; deps: string[] }
export interface FileRow { path: string; kind: string; domain: string; route: string }
export interface CodeBlast {
  target: string;
  measured: true;
  units: { direct: string[]; through: string[] };
  services: { direct: string[]; through: string[] };
  domains: { name: string; files: number }[];
  tests: number;
  pages: string[];
}
export interface Unmeasured { target: string; measured: false; reason: string }

/** The unit a repo path sits in: the longest unitPath that is the path or its prefix. */
export function unitOf(path: string, units: UnitRow[]): UnitRow | undefined {
  return units
    // #4405 — the root unit (".") holds every file no deeper unit does
    .filter((u) => u.path === '.' || path === u.path || path.startsWith(`${u.path}/`))
    .sort((a, b) => b.path.length - a.path.length)[0];
}

/** Every unit depending on any of `start`, transitively, nearest first. */
export function dependentsOf(start: string[], units: UnitRow[]): string[] {
  const out: string[] = [];
  const queue = [...start];
  while (queue.length) {
    const n = queue.shift() as string;
    for (const u of units) {
      if (u.deps.includes(n) && !start.includes(u.name) && !out.includes(u.name)) {
        out.push(u.name);
        queue.push(u.name);
      }
    }
  }
  return out;
}

/**
 * The answer for a repo path (file or unit) or a service (`service-x` or
 * `chorus:service-x`). Unmeasured — never "no blast radius" — when the target
 * is in no unit or the store holds no units at all.
 */
export function codeBlast(
  target: string, units: UnitRow[], builtFrom: [string, string][], files: FileRow[],
): CodeBlast | Unmeasured {
  if (!units.length) return { target, measured: false, reason: 'the store holds no code units yet (the crawl has not run on this tree)' };
  let direct: string[];
  const svc = target.replace(/^chorus:/, '');
  if (svc.startsWith('service-') && !target.includes('/')) {
    direct = [...new Set(builtFrom.filter(([s]) => s === svc).map(([, u]) => u))];
    if (!direct.length) return { target, measured: false, reason: `${svc} names no code unit it is built from` };
  } else {
    const u = unitOf(target.replace(/^\/+|\/+$/g, ''), units);
    if (!u) return { target, measured: false, reason: `${target} is in no code unit` };
    direct = [u.name];
  }
  const through = dependentsOf(direct, units);
  const all = [...direct, ...through];
  // a file counts for the unit it belongs to (its deepest unit), so a root
  // unit never claims files that sit in a deeper one
  const inScope = files.filter((f) => { const own = unitOf(f.path, units); return !!own && all.includes(own.name); });
  const servicesOf = (us: string[]): string[] => [...new Set(builtFrom.filter(([, u]) => us.includes(u)).map(([s]) => s))].sort();
  const directSvc = servicesOf(direct);
  const domains = new Map<string, number>();
  for (const f of inScope) if (f.domain) domains.set(f.domain, (domains.get(f.domain) ?? 0) + 1);
  return {
    target,
    measured: true,
    units: { direct, through },
    services: { direct: directSvc, through: servicesOf(through).filter((s) => !directSvc.includes(s)) },
    domains: [...domains].map(([name, n]) => ({ name, files: n })).sort((a, b) => b.files - a.files || a.name.localeCompare(b.name)),
    tests: inScope.filter((f) => f.kind === 'test').length,
    pages: [...new Set(inScope.filter((f) => f.route).map((f) => f.route))].sort(),
  };
}

export function unitsQuery(): string {
  return `SELECT ?u ?path ?kind ?dep ?repo WHERE { GRAPH ?g { ?u a <${C}CodeUnit> ; <${C}unitPath> ?path ; <${C}unitKind> ?kind . OPTIONAL { ?u <${C}dependsOn> ?dep } OPTIONAL { ?u <${C}inRepository> ?repo } } }`;
}
export function builtFromQuery(): string {
  return `SELECT ?s ?u WHERE { GRAPH ?g { ?s <${C}builtFrom> ?u } }`;
}
/**
 * Code files only, from the code graph: the tests graph holds ~900k test
 * results that also carry filePath, and a GRAPH ?g read of them ran past 60s
 * (measured on the #4499 run 2 demo, 2026-10-11).
 */
export function filesQuery(): string {
  return `SELECT ?path ?kind ?domain ?route WHERE { GRAPH <urn:chorus:domains:code> { ?f a <${C}CodeFile> ; <${C}filePath> ?path . OPTIONAL { ?f <${C}hasKind> ?kind } OPTIONAL { ?f <${C}hasDomain> ?domain } OPTIONAL { ?f <${C}route> ?route } } }`;
}

/**
 * Pages are their own rows (chorus:Page, a CodeFile child typed only as Page),
 * so the files query never sees their routes: read them apart and fold them in.
 */
export function pagesQuery(): string {
  return `SELECT ?path ?route WHERE { GRAPH <urn:chorus:domains:code> { ?f a <${C}Page> ; <${C}filePath> ?path ; <${C}route> ?route } }`;
}
export function withPages(files: FileRow[], pages: [string, string][]): FileRow[] {
  const out = files.map((f) => ({ ...f }));
  for (const [path, route] of pages) {
    const hit = out.find((f) => f.path === path);
    if (hit) hit.route = route;
    else out.push({ path, kind: '', domain: '', route });
  }
  return out;
}

/** SPARQL CSV rows as string arrays (header dropped); quoted cells unwrapped. */
export function csvRows(csv: string): string[][] {
  return csv.split('\n').slice(1).filter((l) => l.trim()).map((l) => {
    const cells: string[] = [];
    let cur = '';
    let q = false;
    for (const ch of l.replace(/\r$/, '')) {
      if (ch === '"') q = !q;
      else if (ch === ',' && !q) { cells.push(cur); cur = ''; } else cur += ch;
    }
    cells.push(cur);
    return cells;
  });
}

/** #4405 — the repository a blast radius is asked in; rows written before units carried one are the home repo's. */
export const HOME_REPO = 'chorus';
export function unitsFromCsv(csv: string, repo: string = HOME_REPO): UnitRow[] {
  const m = new Map<string, UnitRow>();
  for (const [u, path, kind, dep, r] of csvRows(csv)) {
    const rowRepo = r ? local(r).replace(/^repository-/, '') : HOME_REPO;
    if (rowRepo !== repo) continue;
    const name = local(u).replace(/^code-unit-/, '');
    const row = m.get(name) ?? { name, path, kind, deps: [] };
    if (dep) {
      const d = local(dep).replace(/^code-unit-/, '');
      if (!row.deps.includes(d)) row.deps.push(d);
    }
    m.set(name, row);
  }
  return [...m.values()];
}
export function builtFromFromCsv(csv: string): [string, string][] {
  return csvRows(csv).map(([s, u]) => [local(s), local(u).replace(/^code-unit-/, '')] as [string, string]);
}
export function pagesFromCsv(csv: string): [string, string][] {
  return csvRows(csv).map(([path = '', route = '']) => [path, route] as [string, string]).filter(([p, r]) => p && r);
}
export function filesFromCsv(csv: string): FileRow[] {
  // a row with an absent OPTIONAL has fewer cells: pad, so every field is a string
  return csvRows(csv).map((cells) => {
    const [path = '', kind = '', domain = '', route = ''] = cells;
    return { path, kind: local(kind).replace(/^code-kind-/, ''), domain: local(domain).replace(/^domain-/, ''), route };
  });
}
