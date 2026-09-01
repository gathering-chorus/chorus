// @test-type: unit — pure fold over CAPTURED bindings; no store, brings its own world.
// #4053 — Jeff: "i dont see field or cross class constraints", and then: not via
// sh:message. The rules must draw STRUCTURALLY, not as a sentence someone wrote.
//
// Counted live in urn:chorus:ontology on 2026-09-01: maxCount 44, pattern 7,
// inversePath 3. The inverse-path three are the strongest rules in the model and
// the atlas drew them as a bare "(inverse path)" with no predicate and no target:
//   Domain  : sh:path [sh:inversePath chorus:hasDomain] ; min 1 ; max 1 ; class Product
//             — EXACTLY ONE Product must claim this Domain
//   Service : sh:path [sh:inversePath chorus:hosts]     ; min 1 ; max 1 ; class Domain
//   Domain  : sh:path [sh:inversePath chorus:inStream]  ; min 1 ; class ValueStreamStep
// A requirement the class cannot satisfy by its own authoring is exactly the kind
// that must be visible.
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const XSD = 'http://www.w3.org/2001/XMLSchema#';
const uri = (l: string) => ({ type: 'uri', value: CH + l });
const int = (v: string) => ({ type: 'literal', value: v, datatype: XSD + 'integer' });
const xsd = (l: string) => ({ type: 'uri', value: XSD + l });
const lit = (v: string) => ({ type: 'literal', value: v });
const bnode = { type: 'bnode', value: 'b0' };
const HOMES = new Map([['Product', 'products'], ['ValueStreamStep', 'value-streams']]);

const ROWS = [
  // the exactly-one-parent rule, as the store returns it
  { domain: uri('domains'), class: uri('Domain'), prop: bnode, invOf: uri('hasDomain'),
    min: int('1'), max: int('1'), rc: uri('Product') },
  // a required-but-unbounded reverse rule
  { domain: uri('domains'), class: uri('Domain'), prop: bnode, invOf: uri('inStream'),
    min: int('1'), rc: uri('ValueStreamStep') },
  // a field with a regex and an explicit single-value cap
  { domain: uri('domains'), class: uri('Domain'), prop: uri('name'),
    min: int('1'), max: int('1'), dt: xsd('string'), pattern: lit('^[a-z][a-z0-9-]*$') },
  // an ordinary optional multi-valued field
  { domain: uri('domains'), class: uri('Domain'), prop: uri('comment'), dt: xsd('string') },
] as unknown as SparqlBinding[];

describe('#4053 the atlas draws the rules, structurally', () => {
  it('an inverse path names the predicate and its target — not a bare "(inverse path)"', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edge = atlas.domains[0].classes[0].edges.find((e) => e.to === 'Product');
    expect(edge?.name).toBe('hasDomain');
    expect(edge?.inverse).toBe(true);
  });

  it('the exactly-one-parent rule reads 1..1, so the strongest constraint is legible', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edge = atlas.domains[0].classes[0].edges.find((e) => e.to === 'Product');
    expect(edge?.multiplicity).toBe('1..1');
  });

  it('a required-unbounded reverse rule keeps its own cardinality', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const edge = atlas.domains[0].classes[0].edges.find((e) => e.to === 'ValueStreamStep');
    expect(edge?.name).toBe('inStream');
    expect(edge?.inverse).toBe(true);
    expect(edge?.multiplicity).toBe('1..*');
  });

  it('a field carries its regex and its real cardinality, not just a required flag', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const name = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'name');
    expect(name?.pattern).toBe('^[a-z][a-z0-9-]*$');
    expect(name?.min).toBe(1);
    expect(name?.max).toBe(1);
  });

  // ── negative proofs ──────────────────────────────────────────────────────

  it('NEGATIVE: a forward edge is NOT marked inverse', () => {
    const rows = [{ domain: uri('domains'), class: uri('Domain'), prop: uri('ownedBy'), min: int('1'), rc: uri('Product') }] as unknown as SparqlBinding[];
    const atlas = buildClassAtlas(rows, HOMES);
    expect(atlas.domains[0].classes[0].edges[0].inverse).toBeUndefined();
  });

  it('NEGATIVE: a field with no pattern has no pattern — the property is not invented', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const comment = atlas.domains[0].classes[0].attributes.find((a) => a.name === 'comment');
    expect(comment?.pattern).toBeUndefined();
    expect(comment?.min).toBe(0);
    expect(comment?.max).toBeNull();
  });

  it('NEGATIVE: strip invOf and the edge degrades to the old unlabelled form — the check can fail', () => {
    const noInv = ROWS.map((r) => { const c = { ...r } as Record<string, unknown>; delete c.invOf; return c; }) as SparqlBinding[];
    const atlas = buildClassAtlas(noInv, HOMES);
    const edge = atlas.domains[0].classes[0].edges.find((e) => e.to === 'Product');
    expect(edge?.name).toBe('(inverse path)');
    // still an inverse edge — direction comes from the blank-node path, not the
    // predicate name — but the reader can no longer tell WHICH rule it is.
    expect(edge?.inverse).toBe(true);
  });

  it('NEGATIVE: two different inverse paths on one class stay two edges, not merged', () => {
    const atlas = buildClassAtlas(ROWS, HOMES);
    const inverses = atlas.domains[0].classes[0].edges.filter((e) => e.inverse);
    expect(inverses.map((e) => e.name).sort()).toEqual(['hasDomain', 'inStream']);
  });
});
