// @test-type: unit
// @domain: code · @card: 4499 · owner: kade
// #4499 — "what breaks if I change X", from the crawler's code units. Jeff,
// 2026-10-11 10:00: "this feels like a volatile data set"; the old answer
// listed 1,748 files for clearing and could not find a service. Wren 16:41:
// services, domains, tests and pages — never a file list — direct, then through
// dependencies, the same for a file and a service.
import { codeBlast, unitsFromCsv, builtFromFromCsv, filesFromCsv, type UnitRow, type FileRow } from '../src/handlers/athena-code-blast';

const C = 'https://jeffbridwell.com/chorus#';
const UNITS = [
  'u,path,kind,dep',
  `${C}code-unit-platform-services-werk-teardown,platform/services/werk-teardown,crate,`,
  `${C}code-unit-platform-services-werk-accept,platform/services/werk-accept,crate,${C}code-unit-platform-services-werk-teardown`,
  `${C}code-unit-platform-services-werk-push,platform/services/werk-push,crate,${C}code-unit-platform-services-werk-teardown`,
  `${C}code-unit-platform-api,platform/api,package,`,
].join('\n');
const BUILT = ['s,u', `${C}service-werk,${C}code-unit-platform-services-werk-accept`, `${C}service-knowledge-store,${C}code-unit-platform-api`].join('\n');
const FILES = [
  'path,kind,domain,route',
  `platform/services/werk-accept/src/lib.rs,${C}code,${C}version-control,`,
  `platform/services/werk-accept/tests/a.rs,${C}test,${C}version-control,`,
  `platform/services/werk-push/src/lib.rs,${C}code,${C}version-control,`,
  `platform/services/werk-teardown/src/lib.rs,${C}code,${C}version-control,`,
  `platform/api/public/werk/index.html,${C}code,${C}cicd,/werk/`,
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
});
