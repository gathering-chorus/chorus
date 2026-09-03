// @test-type: unit — reads the shipped page sources; no server, no network
// #4093 — the service page composes its hosting domain's facets through ONE renderer
// (facets.js). The defect this guards: a second copy of the facet table on a page,
// which is how the domain page and the service page would drift apart.
import * as fs from 'fs';
import * as path from 'path';

const PUB = path.join(__dirname, '..', 'public', 'athena');
const read = (f: string) => fs.readFileSync(path.join(PUB, f), 'utf8');

/** The rule: a page may USE the facet table, never DEFINE it. */
function facetTableDefinitions(src: string): number {
  return (src.match(/const\s+FACETS\s*=\s*\[/g) || []).length;
}

describe('#4093 service page engineering half', () => {
  test('facets.js is the one place the facet table is defined', () => {
    expect(facetTableDefinitions(read('facets.js'))).toBe(1);
    expect(facetTableDefinitions(read('domain.html'))).toBe(0);
    expect(facetTableDefinitions(read('service.html'))).toBe(0);
  });

  test('both pages load the shared renderer', () => {
    expect(read('domain.html')).toContain('facets.js');
    expect(read('service.html')).toContain('facets.js');
  });

  test('the service page composes the domain facets and adds only what a service owns', () => {
    const s = read('service.html');
    for (const chapter of ['Flows', 'Runs as', 'Commitments']) expect(s).toContain(chapter);
    expect(s).toContain("renderFacetTables(");
    expect(s).toContain("'API Contract', 'Persistence', 'Dependencies', 'Tests'");
    // the hosting domain is asked of the read-only SPARQL door, never guessed from the name
    expect(s).toContain('chorus:hosts chorus:${s}');
  });

  test('every chapter tells an empty model apart from a fetch failure', () => {
    const s = read('service.html');
    expect(s).toContain('No diagram on this service yet');
    expect(s).toContain('No harvested unit names this service');
    expect(s).toContain('No domain hosts this service in the model');
    expect((s.match(/Cannot reach the (model|hosting domain)/g) || []).length).toBeGreaterThanOrEqual(3);
  });

  test('NEGATIVE PROOF: a page that defines its own facet table is caught', () => {
    const rogue = read('service.html') + "\n<script>const FACETS = [ { t: 'Cards' } ];</script>";
    expect(facetTableDefinitions(rogue)).toBe(1);
  });
});
