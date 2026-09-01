// @test-type: unit — pure fold over CAPTURED bindings; no store, brings its own world.
// #4053 — Jeff: "we need a definition of each attribute and the class this is the
// semantic layer." The structural layer was already drawn; the semantic one was
// absent, so the atlas could say a field exists but never what it means.
//
// Measured live 2026-09-01: rdfs:comment covers 95/135 classes, 118/157 datatype
// properties, 84/122 object properties. Product 6/13 fields defined, Domain 9/13,
// Service 5/14 — and `gaps` is REQUIRED on all three and defined on none. The
// coverage gap is the point: missing definitions must be visible, not hidden,
// which is why `definedCount` is a count the fold reports rather than a boolean.
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const XSD = 'http://www.w3.org/2001/XMLSchema#';
const uri = (l: string) => ({ type: 'uri', value: CH + l });
const int = (v: string) => ({ type: 'literal', value: v, datatype: XSD + 'integer' });
const xsd = (l: string) => ({ type: 'uri', value: XSD + l });
const lit = (v: string) => ({ type: 'literal', value: v });
const HOMES = new Map([['Domain', 'domains']]);

// Definitions verbatim from the store; `gaps` and `label` genuinely have none.
const PRODUCT_DEF = 'A system being built by the team. Products are outputs of the spine AND inputs back to Capturing (feedback loop).';
const HASDOMAIN_DEF = 'True containment — a product has this domain as a constituent part. Loom hasDomain Principles.';

const ROWS = [
  { domain: uri('products'), class: uri('Product'), classDef: lit(PRODUCT_DEF),
    prop: uri('hasDomain'), propDef: lit(HASDOMAIN_DEF), min: int('1'), rc: uri('Domain') },
  { domain: uri('products'), class: uri('Product'), classDef: lit(PRODUCT_DEF),
    prop: uri('gaps'), min: int('1'), dt: xsd('string') },
  { domain: uri('products'), class: uri('Product'), classDef: lit(PRODUCT_DEF),
    prop: uri('label'), min: int('1'), dt: xsd('string') },
] as unknown as SparqlBinding[];

describe('#4053 the atlas carries the semantic layer', () => {
  it('the class carries its own definition', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    expect(atlas.domains[0].classes[0].definition).toBe(PRODUCT_DEF);
  });

  it('an edge carries the definition of its property', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edge = atlas.domains[0].classes[0].edges.find((e) => e.name === 'hasDomain');
    expect(edge?.definition).toBe(HASDOMAIN_DEF);
  });

  it('an attribute with no rdfs:comment reports NO definition rather than an empty string', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const gaps = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'gaps');
    expect(gaps).toBeDefined();
    expect(gaps?.definition).toBeUndefined();
  });

  it('the class reports how many of its members are defined, so the gap is countable', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const cls = atlas.domains[0].classes[0];
    // 3 members: hasDomain defined, gaps and label not.
    expect(cls.definedCount).toBe(1);
    expect(cls.memberCount).toBe(3);
  });

  // ── negative proofs ──────────────────────────────────────────────────────

  it('NEGATIVE: a class with no rdfs:comment has no definition — never a placeholder', () => {
    const rows = [{ domain: uri('x'), class: uri('Undefined'), prop: uri('f'), min: int('1'), dt: xsd('string') }] as unknown as SparqlBinding[];
    const atlas = buildClassAtlas(rows, HOMES);
    expect(atlas.domains[0].classes[0].definition).toBeUndefined();
    expect(atlas.domains[0].classes[0].definedCount).toBe(0);
  });

  it('NEGATIVE: strip every definition and definedCount goes to zero — the count can fail', () => {
    const stripped = ROWS.map((r) => {
      const c = { ...r } as Record<string, unknown>;
      delete c.classDef; delete c.propDef; return c;
    }) as SparqlBinding[];
    const cls = buildClassAtlas(stripped, HOMES).domains[0].classes[0];
    expect(cls.definedCount).toBe(0);
    expect(cls.memberCount).toBe(3);
    expect(cls.definition).toBeUndefined();
  });

  it('NEGATIVE: a definition on one property does not leak onto another', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const label = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'label');
    expect(label?.definition).toBeUndefined();
  });
});
