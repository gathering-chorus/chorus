// @test-type: unit — pure fold over captured bindings; no store, brings its own world.
// #4358 — Jeff's atlas screenshot, 2026-10-08 08:52: Principle "4/7 defined";
// label, comment and order read "no definition — rdfs:comment missing". Their
// shape describes each one (sh:description), but the atlas only read the
// property's rdfs:comment, and rdfs:label / rdfs:comment are W3C properties we
// do not annotate. The field's shape description is now its definition.
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const RDFS = 'http://www.w3.org/2000/01/rdf-schema#';
const uri = (v: string) => ({ type: 'uri', value: v });
const lit = (v: string) => ({ type: 'literal', value: v });
const HOMES = new Map([['Principle', 'principles']]);
const base = { domain: uri(CH + 'principles'), class: uri(CH + 'Principle') };

const LABEL_DESC = "Required. The principle's name as Hemenway words it, e.g. Observe.";
const READING_COMMENT = 'The Jeff lens reading of a principle.';
const READING_DESC = "Required. What the principle means in how Jeff works and runs the team.";

describe('#4358 a field is defined by its shape description', () => {
  it('rdfs:label, with no rdfs:comment of its own, takes its shape description', () => {
    const atlas = buildClassAtlas([{ ...base, prop: uri(RDFS + 'label'), shapeDef: lit(LABEL_DESC) }] as unknown as SparqlBinding[], HOMES);
    const cls = atlas.domains[0].classes[0];
    expect(cls.attributes[0].definition).toBe(LABEL_DESC);
    expect(cls.definedCount).toBe(1);
  });

  it('the shape description wins over the property\'s global comment', () => {
    const atlas = buildClassAtlas([{ ...base, prop: uri(CH + 'jeffReading'), propDef: lit(READING_COMMENT), shapeDef: lit(READING_DESC) }] as unknown as SparqlBinding[], HOMES);
    expect(atlas.domains[0].classes[0].attributes[0].definition).toBe(READING_DESC);
  });

  it('NEGATIVE PROOF: a field with neither stays undefined and is not counted', () => {
    const atlas = buildClassAtlas([{ ...base, prop: uri(CH + 'order') }] as unknown as SparqlBinding[], HOMES);
    const cls = atlas.domains[0].classes[0];
    expect(cls.attributes[0].definition).toBeUndefined();
    expect(cls.definedCount).toBe(0);
  });
});
