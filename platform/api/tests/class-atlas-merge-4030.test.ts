// @test-type: unit — pure function over fixture bindings; brings its own world.
// #4030 — Class Atlas row merging (#3992). Jeff's experience under test: a
// property that SPARQL returns on several OPTIONAL rows shows once; an inverse
// path is shown and labelled rather than dropped; rows without a property still
// register the class.
import { buildClassAtlas } from '../src/handlers/class-atlas';

const NS = 'https://jeffbridwell.com/chorus#';
const u = (v: string) => ({ value: `${NS}${v}` });

describe('class atlas merging (#4030)', () => {
  it('dedupes repeated edge and attribute rows, labels inverse paths, keeps property-less classes', () => {
    const rows = [
      { domain: u('domain-board'), class: u('Card') },                                   // no prop: class only
      { domain: u('domain-board'), class: u('Card'), prop: u('ownedBy'), rc: u('Role') },
      { domain: u('domain-board'), class: u('Card'), prop: u('ownedBy'), rc: u('Role') },  // duplicate edge
      { domain: u('domain-board'), class: u('Card'), prop: u('title'), dt: { value: 'http://www.w3.org/2001/XMLSchema#string' } },
      { domain: u('domain-board'), class: u('Card'), prop: u('title'), dt: { value: 'http://www.w3.org/2001/XMLSchema#string' } }, // duplicate attribute
      { domain: u('domain-board'), class: u('Card'), prop: { type: 'bnode', value: 'b0' }, rc: u('Chunk') },
    ];
    const atlas = buildClassAtlas(rows as any, new Map([['Role', 'domain-roles'], ['Chunk', 'domain-board']]));
    const card = atlas.domains[0].classes[0];
    expect(card.name).toBe('Card');
    expect(card.edges).toEqual([
      { name: 'ownedBy', to: 'Role', multiplicity: '0..*', crossDomain: true },
      // #4053 — a blank-node path is now flagged `inverse`, so a reverse
      // requirement is distinguishable from a forward one.
      { name: '(inverse path)', to: 'Chunk', multiplicity: '0..*', crossDomain: false, inverse: true },
    ]);
    expect(card.attributes).toEqual([{ name: 'title', type: 'string', min: 0, max: null }]);
  });
});
