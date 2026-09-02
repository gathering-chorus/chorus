// @test-type: unit — pure fold over CAPTURED bindings; no store, brings its own world.
// #4053 AC7 — Jeff: "there is no dependency from products class to services class
// at all." chorus:consumes was authored 79x and governed by no shape, so nothing
// served it and the atlas could not draw it. Silas ruled consumes is ONE verb
// with a polymorphic target, so ProductShape declares it with sh:or over
// (Service, Domain) — which means the atlas must read sh:or, not just sh:class,
// or the edge stays invisible and the model change buys nothing on the page.
//
// Bindings CAPTURED 2026-09-01 from the shape of the sh:or walk:
//   ?b sh:or ?l . ?l rdf:rest*/rdf:first ?alt . ?alt sh:class ?orClass
// which yields ONE ROW PER ALTERNATIVE.
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const uri = (l: string) => ({ type: 'uri', value: CH + l });
const int = (v: string) => ({ type: 'literal', value: v, datatype: 'http://www.w3.org/2001/XMLSchema#integer' });
const HOMES = new Map([['Service', 'services'], ['Domain', 'domains'], ['Document', 'documents']]);

const ROWS = [
  // the plain sh:class edge, unchanged
  { domain: uri('products'), class: uri('Product'), prop: uri('hasDomain'), min: int('1'), rc: uri('Domain') },
  // consumes: one row per sh:or alternative
  { domain: uri('products'), class: uri('Product'), prop: uri('consumes'), min: int('0'), orClass: uri('Service') },
  { domain: uri('products'), class: uri('Product'), prop: uri('consumes'), min: int('0'), orClass: uri('Domain') },
  { domain: uri('products'), class: uri('Product'), prop: uri('hasServiceDesign'), min: int('0'), rc: uri('Document') },
] as unknown as SparqlBinding[];

describe('#4053 the atlas draws sh:or edges — Product reaches Service', () => {
  it('Product has an edge to Service, which is the whole of Jeff\'s finding', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edges = atlas.domains[0].classes[0].edges;
    expect(edges.some((e) => e.name === 'consumes' && e.to === 'Service')).toBe(true);
  });

  it('both alternatives draw — a polymorphic target shows every branch', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const consumes = atlas.domains[0].classes[0].edges.filter((e) => e.name === 'consumes');
    expect(consumes.map((e) => e.to).sort()).toEqual(['Domain', 'Service']);
  });

  it('an sh:or edge is optional-cardinality and cross-domain, like any other', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const toService = atlas.domains[0].classes[0].edges.find((e) => e.to === 'Service');
    expect(toService?.multiplicity).toBe('0..*');
    expect(toService?.crossDomain).toBe(true);
  });

  it('sh:class edges still work — the change is additive', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edges = atlas.domains[0].classes[0].edges;
    expect(edges.some((e) => e.name === 'hasDomain' && e.to === 'Domain')).toBe(true);
    expect(edges.some((e) => e.name === 'hasServiceDesign' && e.to === 'Document')).toBe(true);
  });

  // ── negative proofs ──────────────────────────────────────────────────────

  it('NEGATIVE: drop the sh:or rows and Product reaches Service NOWHERE — the pre-fix state', () => {
    const withoutOr = ROWS.filter((r) => !(r as { orClass?: unknown }).orClass);
    const atlas = buildClassAtlas(withoutOr, HOMES);
    const edges = atlas.domains[0].classes[0].edges;
    expect(edges.some((e) => e.to === 'Service')).toBe(false);
    expect(edges.some((e) => e.name === 'consumes')).toBe(false);
  });

  it('NEGATIVE: an sh:or alternative must not become an attribute', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    expect(atlas.domains[0].classes[0].attributes.some((a) => a.name === 'consumes')).toBe(false);
  });

  it('NEGATIVE: repeated alternative rows do not duplicate the edge', () => {
    const atlas = buildClassAtlas([...ROWS, ROWS[1], ROWS[2]] as SparqlBinding[], HOMES);
    const consumes = atlas.domains[0].classes[0].edges.filter((e) => e.name === 'consumes');
    expect(consumes).toHaveLength(2);
  });
});
