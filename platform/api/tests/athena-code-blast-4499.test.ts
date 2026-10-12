// @test-type: unit
// @domain: code · @card: 4499 · owner: kade
// #4499 — "what breaks if I change X", from the crawler's code units. Jeff,
// 2026-10-11 10:00: "this feels like a volatile data set"; the old answer
// listed 1,748 files for clearing and could not find a service. Wren 16:41:
// services, domains, tests and pages — never a file list — direct, then through
// dependencies, the same for a file and a service.
import { codeBlast, filesQuery, withPages, pagesFromCsv, unitsFromCsv, builtFromFromCsv, filesFromCsv, repoEdgesFromCsv, dependentRepos, type UnitRow, type FileRow } from '../src/handlers/athena-code-blast';

const C = 'https://jeffbridwell.com/chorus#';
const UNITS = [
  'u,path,kind,dep,repo',
  `${C}code-unit-platform-services-werk-teardown,platform/services/werk-teardown,crate,,${C}repository-chorus`,
  `${C}code-unit-platform-services-werk-accept,platform/services/werk-accept,crate,${C}code-unit-platform-services-werk-teardown,${C}repository-chorus`,
  `${C}code-unit-platform-services-werk-push,platform/services/werk-push,crate,${C}code-unit-platform-services-werk-teardown,${C}repository-chorus`,
  `${C}code-unit-platform-api,platform/api,package,,${C}repository-chorus`,
].join('\n');
const BUILT = ['s,u', `${C}service-werk,${C}code-unit-platform-services-werk-accept`, `${C}service-knowledge-store,${C}code-unit-platform-api`].join('\n');
const FILES = [
  'path,kind,domain,route,repo',
  `platform/services/werk-accept/src/lib.rs,${C}code,${C}version-control,,${C}repository-chorus`,
  `platform/services/werk-accept/tests/a.rs,${C}test,${C}version-control,,${C}repository-chorus`,
  `platform/services/werk-push/src/lib.rs,${C}code,${C}version-control,,${C}repository-chorus`,
  `platform/services/werk-teardown/src/lib.rs,${C}code,${C}version-control,,${C}repository-chorus`,
  `platform/api/public/werk/index.html,${C}code,${C}cicd,/werk/,${C}repository-chorus`,
].join('\n');

const units: UnitRow[] = unitsFromCsv(UNITS);
const built = builtFromFromCsv(BUILT);
const files: FileRow[] = filesFromCsv(FILES);

describe('#4499 the blast radius of a change, from the code units', () => {
  it('a file resolves to its unit, then everything depending on it', () => {
    const r = codeBlast('platform/services/werk-teardown/src/lib.rs', units, built, files);
    if (!r.measured) throw new Error(r.reason);
    expect(r.units.direct).toEqual(['platform-services-werk-teardown']);
    expect(r.units.through.sort()).toEqual(['platform-services-werk-accept', 'platform-services-werk-push']);
    expect(r.services).toEqual({ direct: [], through: ['service-werk'] });
    expect(r.domains).toEqual([{ name: 'version-control', files: 4 }]);
    expect(r.tests).toBe(1);
  });

  it('a service answers the same shape, from what it is built from', () => {
    const r = codeBlast('chorus:service-knowledge-store', units, built, files);
    if (!r.measured) throw new Error(r.reason);
    expect(r.services.direct).toEqual(['service-knowledge-store']);
    expect(r.pages).toEqual(['/werk/']);
    expect(r.domains).toEqual([{ name: 'cicd', files: 1 }]);
  });

  it('never answers with a file list', () => {
    const r = codeBlast('platform/services/werk-accept', units, built, files);
    expect(JSON.stringify(r)).not.toContain('src/lib.rs');
  });

  it('NEGATIVE PROOF — a file in no unit is UNMEASURED, never "no blast radius"', () => {
    const r = codeBlast('designing/docs/x.html', units, built, files);
    expect(r).toEqual({ target: 'designing/docs/x.html', measured: false, reason: 'designing/docs/x.html is in no code unit' });
  });

  it('NEGATIVE PROOF — a store with no units yet is UNMEASURED, not empty', () => {
    expect(codeBlast('platform/api/src/server.ts', [], built, files).measured).toBe(false);
  });

  it('NEGATIVE PROOF — files come from the code graph, never every graph (the tests graph holds ~900k filePath rows)', () => {
    const q = filesQuery();
    expect(q).toContain('GRAPH <urn:chorus:domains:code>');
    expect(q).toContain('CodeFile>');
    expect(q).not.toContain('GRAPH ?g');
  });

  // #4499 reopen — pages came back 0 for every target: Page rows are typed only
  // as Page, so the CodeFile read never carried a route.
  it('the pages a change reaches come from the Page rows', () => {
    const pages = pagesFromCsv(`path,route,repo\nplatform/api/public/athena/x.html,/athena/x.html,${C}repository-chorus\ndirecting/elsewhere/y.html,/y.html,${C}repository-chorus\n`);
    const codeFiles = files.map((f) => ({ ...f, route: '' })); // live CodeFile rows carry no route
    const r = codeBlast('platform/api/src/server.ts', units, built, withPages(codeFiles, pages));
    expect(r.measured && r.pages).toEqual(['/athena/x.html']);
  });

  it('NEGATIVE PROOF — without the Page rows no page is reported', () => {
    const r = codeBlast('platform/api/src/server.ts', units, built, files.map((f) => ({ ...f, route: '' })));
    expect(r.measured && r.pages).toEqual([]);
  });
});

// #4405 — units carry their repository; a blast radius is asked in one repo
describe('#4405 code-blast reads one repository', () => {
  const MIXED = [
    'u,path,kind,dep,repo',
    `${C}code-unit-chorus--src,src,package,,${C}repository-chorus`,
    `${C}code-unit-wordpress-blog--src,src,package,,${C}repository-wordpress-blog`,
    `${C}code-unit-platform-api,platform/api,package,,`,
  ].join('\n');
  it('chorus by default, and only rows that name chorus', () => {
    expect(unitsFromCsv(MIXED).map((u) => u.name)).toEqual(['chorus--src']);
  });
  it('NEGATIVE PROOF — a unit that names no repository answers for no repository (#4405 reopen)', () => {
    expect(unitsFromCsv(MIXED, 'chorus').map((u) => u.name)).not.toContain('platform-api');
    expect(unitsFromCsv(MIXED, 'wordpress-blog').map((u) => u.name)).not.toContain('platform-api');
  });
  it("NEGATIVE PROOF — another repo's unit with the same path never answers for the home repo", () => {
    const r = codeBlast('src/index.ts', unitsFromCsv(MIXED), [], []);
    expect(r.measured && r.units.direct).toEqual(['chorus--src']);
    expect(unitsFromCsv(MIXED, 'wordpress-blog').map((u) => u.name)).toEqual(['wordpress-blog--src']);
  });
});

describe('#4405 a root build file is a unit', () => {
  const ROOTED: UnitRow[] = [
    { name: 'site--root', path: '.', kind: 'package', deps: [] },
    { name: 'site--media-server', path: 'media-server', kind: 'package', deps: [] },
  ];
  const F: FileRow[] = [
    { path: 'src/server.ts', kind: 'code', domain: 'web', route: '' },
    { path: 'media-server/index.js', kind: 'code', domain: 'media', route: '' },
  ];
  it('a file in no deeper unit belongs to the root unit', () => {
    const r = codeBlast('src/server.ts', ROOTED, [], F);
    expect(r.measured && r.units.direct).toEqual(['site--root']);
  });
  it("NEGATIVE PROOF — the root unit never claims a deeper unit's files", () => {
    const r = codeBlast('src/server.ts', ROOTED, [], F);
    expect(r.measured && r.domains).toEqual([{ name: 'web', files: 1 }]);
  });
});

// #4405 reopen — repositories depend on repositories (Jeff, 2026-10-12: "chorus
// repo depends on shared-security and shared-observability and gathering"); a
// change in one names every repository that needs it, transitively.
describe('#4405 repositories a change reaches', () => {
  const EDGES = [
    'r,dep',
    `${C}repository-chorus,${C}repository-shared-security`,
    `${C}repository-chorus,${C}repository-jeff-bridwell-personal-site`,
    `${C}repository-wine-diary,${C}repository-shared-security`,
    `${C}repository-ops-console,${C}repository-chorus`,
  ].join('\n');
  it('a shared-security change names chorus, wine-diary, and what depends on chorus', () => {
    expect(dependentRepos('shared-security', repoEdgesFromCsv(EDGES))).toEqual(['chorus', 'wine-diary', 'ops-console']);
  });
  it('NEGATIVE PROOF — a repository nothing depends on reaches no other', () => {
    expect(dependentRepos('wine-diary', repoEdgesFromCsv(EDGES))).toEqual([]);
  });
  it('the answer carries the repository and its dependents', () => {
    const units: UnitRow[] = [{ name: 'shared-security--root', path: '.', kind: 'package', deps: [] }];
    const r = codeBlast('bin/start.sh', units, [], [], { repo: 'shared-security', edges: repoEdgesFromCsv(EDGES) });
    expect(r.measured && r.repositories).toEqual({ name: 'shared-security', dependents: ['chorus', 'wine-diary', 'ops-console'] });
  });
});

// #4405 reopen 3 — files carry their repository. Before, another repo's root
// unit claimed every chorus file: shared-security answered 1,332 tests, 21 pages.
describe('#4405 files answer for their own repository', () => {
  const MIXED_FILES = [
    'path,kind,domain,route,repo',
    `src/index.ts,${C}code,${C}cards,,${C}repository-chorus`,
    `tests/a.test.ts,${C}test,${C}cards,,${C}repository-chorus`,
    `bin/start.sh,${C}code,${C}identity,,${C}repository-shared-security`,
    `old/no-repo.ts,${C}test,${C}cards,,`,
  ].join('\n');
  const root: UnitRow[] = [{ name: 'shared-security-root', path: '.', kind: 'package', deps: [] }];
  it("NEGATIVE PROOF — another repo's root unit never claims chorus's files", () => {
    const r = codeBlast('bin/start.sh', root, [], filesFromCsv(MIXED_FILES, 'shared-security'));
    expect(r.measured && [r.tests, r.domains]).toEqual([0, [{ name: 'identity', files: 1 }]]);
  });
  it('a file that names no repository answers for none', () => {
    expect(filesFromCsv(MIXED_FILES, 'chorus').map((f) => f.path)).toEqual(['src/index.ts', 'tests/a.test.ts']);
  });
  it('a page answers only for its own repository', () => {
    const pages = pagesFromCsv(`path,route,repo\na.html,/a,${C}repository-chorus\nb.html,/b,${C}repository-shared-security\n`, 'shared-security');
    expect(pages).toEqual([['b.html', '/b']]);
  });
});
