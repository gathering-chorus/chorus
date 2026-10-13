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
  /** #4405 reopen — the repository asked in, and every repository that depends on it */
  repositories?: { name: string; dependents: string[] };
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

/** #4405 reopen — every repository that depends on `repo`, transitively, nearest first. */
export function dependentRepos(repo: string, edges: [string, string][]): string[] {
  const out: string[] = [];
  const queue = [repo];
  while (queue.length) {
    const n = queue.shift() as string;
    for (const [r, dep] of edges) {
      if (dep === n && r !== repo && !out.includes(r)) {
        out.push(r);
        queue.push(r);
      }
    }
  }
  return out;
}

/**
 * #4499 reopen 3 — what runs: one row per ServiceInstance or ScheduledJob that
 * names a service, with the repo path of the file it runs (when the crawl has
 * that file) and the binary launchd starts.
 */
export interface RunRow { service: string; path: string; binary: string }
export interface Running { services: string[]; runs: RunRow[] }

/** The unit a bare name means: its door name, or the last segment of its path, when exactly one unit has it. */
export function unitNamed(name: string, units: UnitRow[]): UnitRow | undefined {
  const exact = units.find((u) => u.name === name);
  if (exact) return exact;
  const byDir = units.filter((u) => u.path.split('/').pop() === name);
  return byDir.length === 1 ? byDir[0] : undefined;
}

/**
 * The answer for a repo path (file or unit), a unit's name, or a service
 * (`service-x` or `chorus:service-x`). Unmeasured — never "no blast radius" —
 * when the store holds no units. #4499 reopen 3: a target the crawl does not
 * know is refused and named, never answered as a root-unit file.
 */
export function codeBlast(
  target: string, units: UnitRow[], builtFrom: [string, string][], files: FileRow[],
  repos?: { repo: string; edges: [string, string][] },
  running?: Running,
): CodeBlast | Unmeasured {
  if (!units.length) return { target, measured: false, reason: 'the store holds no code units yet (the crawl has not run on this tree)' };
  const bare = target.replace(/^chorus:/, '');
  const asService = !target.includes('/') && (bare.startsWith('service-') || !!running?.services.includes(`service-${bare}`)) && !unitNamed(bare, units);
  const svc = asService ? (bare.startsWith('service-') ? bare : `service-${bare}`) : '';
  let direct: string[];
  let runBy: string[] = [];
  if (asService) {
    if (running && !running.services.includes(svc)) return { target, measured: false, reason: `there is no service named ${svc}` };
    direct = [...new Set(builtFrom.filter(([s]) => s === svc).map(([, u]) => u))];
    if (!direct.length) {
      const mine = (running?.runs ?? []).filter((r) => r.service === svc);
      if (!mine.length) return { target, measured: false, reason: `${svc} names no code unit it is built from and nothing runs it` };
      // say what it does run: files in no crate or package, or code outside this repository
      const loose = [...new Set(mine.filter((r) => r.path).map((r) => r.path))].sort();
      const outside = [...new Set(mine.filter((r) => !r.path && r.binary).map((r) => r.binary))].sort();
      const said = [loose.length ? `files in no crate or package: ${loose.join(', ')}` : '', outside.length ? `code the crawl does not hold: ${outside.join(', ')}` : ''].filter(Boolean);
      return { target, measured: false, reason: `${svc} is built from no code unit; it runs ${said.join('; and ') || 'nothing it names'}` };
    }
  } else if (!target.includes('/')) {
    const u = unitNamed(bare, units);
    if (!u) return { target, measured: false, reason: `${target} names no file, code unit or service` };
    direct = [u.name];
  } else {
    const path = target.replace(/^\/+|\/+$/g, '');
    const asUnit = units.find((u) => u.path === path);
    const known = asUnit || files.some((f) => f.path === path || f.path.startsWith(`${path}/`));
    if (!known) return { target, measured: false, reason: `${target} is no file or folder the crawl holds` };
    const u = asUnit ?? unitOf(path, units);
    if (!u) return { target, measured: false, reason: `${target} is in no code unit` };
    direct = [u.name];
    // a file only the root unit holds: what runs it is its blast radius too
    if (u.path === '.') {
      runBy = [...new Set((running?.runs ?? []).filter((r) => r.path === path || r.path.startsWith(`${path}/`)).map((r) => r.service))].sort();
    }
  }
  const through = dependentsOf(direct, units);
  const all = [...direct, ...through];
  // a file counts for the unit it belongs to (its deepest unit), so a root
  // unit never claims files that sit in a deeper one
  const inScope = files.filter((f) => { const own = unitOf(f.path, units); return !!own && all.includes(own.name); });
  const servicesOf = (us: string[]): string[] => [...new Set(builtFrom.filter(([, u]) => us.includes(u)).map(([s]) => s))].sort();
  const directSvc = [...new Set([...servicesOf(direct), ...runBy])].sort();
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
    ...(repos ? { repositories: { name: repos.repo, dependents: dependentRepos(repos.repo, repos.edges) } } : {}),
  };
}

export function unitsQuery(): string {
  return `SELECT ?u ?path ?kind ?dep ?repo WHERE { GRAPH ?g { ?u a <${C}CodeUnit> ; <${C}unitPath> ?path ; <${C}unitKind> ?kind . OPTIONAL { ?u <${C}dependsOn> ?dep } OPTIONAL { ?u <${C}inRepository> ?repo } } }`;
}
export function repositoriesQuery(): string {
  return `SELECT ?r ?dep WHERE { GRAPH <urn:chorus:domains:code> { ?r a <${C}Repository> ; <${C}dependsOn> ?dep } }`;
}
export function repoEdgesFromCsv(csv: string): [string, string][] {
  const name = (iri: string): string => local(iri).replace(/^repository-/, '');
  return csvRows(csv).filter(([r, d]) => r && d).map(([r, d]) => [name(r), name(d)] as [string, string]);
}
/** #4499 reopen 3 — every Service, and what each instance or job runs (its file's repo path, its binary). */
export function servicesQuery(): string {
  return `SELECT ?s WHERE { GRAPH ?g { ?s a <${C}Service> } }`;
}
export function runsQuery(): string {
  return `SELECT ?s ?path ?bin WHERE { GRAPH ?g { ?i <${C}runsService> ?s . OPTIONAL { ?i <${C}binaryPath> ?bin } } OPTIONAL { GRAPH ?h { ?i <${C}sourcePath> ?f } GRAPH <urn:chorus:domains:code> { ?f <${C}filePath> ?path } } }`;
}
export function runningFromCsv(services: string, runs: string): Running {
  return {
    services: csvRows(services).map(([s]) => local(s)).filter(Boolean),
    runs: csvRows(runs).map(([s = '', path = '', binary = '']) => ({ service: local(s), path, binary })).filter((r) => r.service),
  };
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
  return `SELECT ?path ?kind ?domain ?route ?repo WHERE { GRAPH <urn:chorus:domains:code> { ?f a <${C}CodeFile> ; <${C}filePath> ?path . OPTIONAL { ?f <${C}hasKind> ?kind } OPTIONAL { ?f <${C}hasDomain> ?domain } OPTIONAL { ?f <${C}route> ?route } OPTIONAL { ?f <${C}inRepository> ?repo } } }`;
}

/**
 * Pages are their own rows (chorus:Page, a CodeFile child typed only as Page),
 * so the files query never sees their routes: read them apart and fold them in.
 */
export function pagesQuery(): string {
  return `SELECT ?path ?route ?repo WHERE { GRAPH <urn:chorus:domains:code> { ?f a <${C}Page> ; <${C}filePath> ?path ; <${C}route> ?route . OPTIONAL { ?f <${C}inRepository> ?repo } } }`;
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

/**
 * #4405 — the repository a blast radius is asked in (chorus when the caller
 * names none). A unit that names no repository answers for none of them.
 */
export const DEFAULT_REPO = 'chorus';
export function unitsFromCsv(csv: string, repo: string = DEFAULT_REPO): UnitRow[] {
  const m = new Map<string, UnitRow>();
  for (const [u, path, kind, dep, r] of csvRows(csv)) {
    if (!r || local(r).replace(/^repository-/, '') !== repo) continue;
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
/** #4405 reopen 3 — a file or page answers only for the repository it names; one that names none answers for none. */
const inRepo = (r: string | undefined, repo: string): boolean => !!r && local(r).replace(/^repository-/, '') === repo;
export function pagesFromCsv(csv: string, repo: string = DEFAULT_REPO): [string, string][] {
  return csvRows(csv)
    .filter(([, , r]) => inRepo(r, repo))
    .map(([path = '', route = '']) => [path, route] as [string, string]).filter(([p, r]) => p && r);
}
export function filesFromCsv(csv: string, repo: string = DEFAULT_REPO): FileRow[] {
  // a row with an absent OPTIONAL has fewer cells: pad, so every field is a string
  return csvRows(csv).filter((cells) => inRepo(cells[4], repo)).map((cells) => {
    const [path = '', kind = '', domain = '', route = ''] = cells;
    return { path, kind: local(kind).replace(/^code-kind-/, ''), domain: local(domain).replace(/^domain-/, ''), route };
  });
}
