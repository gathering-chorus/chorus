// @test-type: unit — pure fold over CAPTURED bindings; no store, brings its own world.
// #4053 — Jeff, looking at the products render: "any status". The class's most
// constrained field drew as its least typed one, because ProductShape declares
// `sh:in ("exploring" "building" "operating" "retiring")` with no sh:datatype
// and the atlas query never asked for sh:in. A viewing surface that omits a
// constraint is worse than no surface — it invites the reader to conclude the
// constraint is absent.
//
// Fixture rows are CAPTURED from the live store on 2026-09-01:
//   POST /pods/query  ?shp sh:targetClass chorus:Product ; sh:property ?b .
//                     ?b sh:path ?prop ; sh:in ?in . ?in rdf:rest*/rdf:first ?member
// which returns ONE ROW PER LIST MEMBER, each member a plain literal — the
// shape the fold has to survive.
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const XSD = 'http://www.w3.org/2001/XMLSchema#';
const uri = (l: string) => ({ type: 'uri', value: CH + l });
const int = (v: string) => ({ type: 'literal', value: v, datatype: XSD + 'integer' });
const xsd = (l: string) => ({ type: 'uri', value: XSD + l });
const lit = (v: string) => ({ type: 'literal', value: v });
const HOMES = new Map([['Domain', 'domains']]);

// products.Product as the store actually returns it: status carries sh:in over
// four members and NO sh:datatype; gaps is a plain required string; hasDomain
// is an edge with minCount and no maxCount (which is why it reads 1..*).
const ROWS = [
  { domain: uri('products'), class: uri('Product'), prop: uri('status'), min: int('1'), inValue: lit('exploring') },
  { domain: uri('products'), class: uri('Product'), prop: uri('status'), min: int('1'), inValue: lit('building') },
  { domain: uri('products'), class: uri('Product'), prop: uri('status'), min: int('1'), inValue: lit('operating') },
  { domain: uri('products'), class: uri('Product'), prop: uri('status'), min: int('1'), inValue: lit('retiring') },
  { domain: uri('products'), class: uri('Product'), prop: uri('gaps'), min: int('1'), dt: xsd('string') },
  { domain: uri('products'), class: uri('Product'), prop: uri('hasDomain'), min: int('1'), rc: uri('Domain') },
] as unknown as SparqlBinding[];

describe('#4053 the atlas shows an enum instead of "any"', () => {
  it('status carries its four allowed values, in declaration order, once each', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const status = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'status');
    expect(status).toBeDefined();
    expect(status?.allowed).toEqual(['exploring', 'building', 'operating', 'retiring']);
  });

  it('an enum attribute reports its type as the enum, never the empty string', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const status = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'status');
    // The defect verbatim: type '' renders as "any status" in the diagram.
    expect(status?.type).not.toBe('');
  });

  it('four member rows collapse to ONE attribute, not four', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const status = atlas.domains[0].classes[0].attributes.filter((a) => a.name === 'status');
    expect(status).toHaveLength(1);
  });

  it('required-ness survives the enum path — status is still min 1', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const status = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'status');
    expect(status?.min).toBe(1);
  });

  // ── negative proofs ──────────────────────────────────────────────────────
  // Without these, `allowed` could be populated by anything and the checks
  // above would still pass — the hollow shape (#3734) in a fixture.

  it('NEGATIVE: a non-enum attribute has no allowed list', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const gaps = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'gaps');
    expect(gaps?.type).toBe('string');
    expect(gaps?.allowed).toBeUndefined();
  });

  it('NEGATIVE: strip the sh:in rows and status disappears — the check can fail', () => {
    const withoutEnum = ROWS.filter((r) => !(r as { inValue?: unknown }).inValue);
    const atlas = buildClassAtlas(withoutEnum, HOMES);
    const status = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'status');
    expect(status).toBeUndefined();
  });

  it('NEGATIVE: sh:in on an EDGE row must not turn a relation into an attribute', () => {
    const rows = [...ROWS, {
      domain: uri('products'), class: uri('Product'), prop: uri('hasDomain'),
      min: int('1'), rc: uri('Domain'), inValue: lit('nonsense'),
    }] as unknown as SparqlBinding[];
    const atlas = buildClassAtlas(rows, HOMES);
    const cls = atlas.domains[0].classes[0];
    expect(cls.edges.filter((e) => e.name === 'hasDomain')).toHaveLength(1);
    expect(cls.attributes.some((a) => a.name === 'hasDomain')).toBe(false);
  });
});
