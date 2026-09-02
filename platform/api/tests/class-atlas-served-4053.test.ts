// @test-type: unit — pure fold over captured shapes; no store, no athena-make, brings its own world.
// #4053 AC4/AC5 — Jeff: the atlas is how he consumes the athena-* pipeline. Today it
// reads urn:chorus:ontology only, so it shows what the PEN declared and cannot show
// whether athena-make actually mounted it or with how many rows. A green verb stays
// unfalsifiable. annotateServed folds athena-make's discovery document and per-
// collection counts onto the atlas so DECLARED and SERVED are distinguishable.
import { annotateServed, Atlas, ServedInfo } from '../src/handlers/class-atlas';

// CAPTURED from GET http://localhost:3360/ on 2026-09-01 — the discovery document
// lists kind + collection per served primitive (37 of them); counts come from each
// collection's own `count` field.
const DISCOVERY: ServedInfo[] = [
  { kind: 'Product', collection: '/v1/products', count: 9 },
  { kind: 'Domain', collection: '/v1/domains', count: 40 },
  { kind: 'SpineEvent', collection: '/v1/spineevents', count: 0 },
];

const ATLAS: Atlas = {
  domains: [
    { name: 'products', classes: [{ name: 'Product', attributes: [], edges: [], parents: [] }] },
    { name: 'events', classes: [{ name: 'SpineEvent', attributes: [], edges: [], parents: [] }] },
    // EventCategory is declared by the spine domain and served by nothing.
    { name: 'spine', classes: [{ name: 'EventCategory', attributes: [], edges: [], parents: [] }] },
  ],
};

const clone = (a: Atlas): Atlas => JSON.parse(JSON.stringify(a)) as Atlas;

describe('#4053 the atlas separates DECLARED from SERVED', () => {
  it('a class athena-make serves is marked served, with its live row count', () => {
    const out = annotateServed(clone(ATLAS), DISCOVERY);
    const product = out.domains.find((d) => d.name === 'products')?.classes[0];
    expect(product?.served).toBe(true);
    expect(product?.rowCount).toBe(9);
  });

  it('a served class with ZERO rows is served AND empty — the two are not the same claim', () => {
    const out = annotateServed(clone(ATLAS), DISCOVERY);
    const spineEvent = out.domains.find((d) => d.name === 'events')?.classes[0];
    // /v1/spineevents is mounted and returns count 0 — a route that serves nothing
    // must not read as a route that works (#3734).
    expect(spineEvent?.served).toBe(true);
    expect(spineEvent?.rowCount).toBe(0);
  });

  it('a declared-but-unserved class is marked unserved, never silently served', () => {
    const out = annotateServed(clone(ATLAS), DISCOVERY);
    const cat = out.domains.find((d) => d.name === 'spine')?.classes[0];
    expect(cat?.served).toBe(false);
    expect(cat?.rowCount).toBeUndefined();
  });

  // ── negative proofs ──────────────────────────────────────────────────────

  it('NEGATIVE: an EMPTY discovery marks everything unserved — never everything served', () => {
    // If discovery cannot be read, the honest answer is "we could not confirm",
    // and the failure direction must be unserved. A fold that defaulted to
    // served would turn an athena-make outage into a clean bill of health.
    const out = annotateServed(clone(ATLAS), []);
    const all = out.domains.flatMap((d) => d.classes);
    expect(all.every((c) => c.served === false)).toBe(true);
    expect(all.some((c) => c.rowCount !== undefined)).toBe(false);
  });

  it('NEGATIVE: discovery for a class the model does not declare adds no phantom class', () => {
    const out = annotateServed(clone(ATLAS), [...DISCOVERY, { kind: 'Ghost', collection: '/v1/ghosts', count: 99 }]);
    const names = out.domains.flatMap((d) => d.classes.map((c) => c.name));
    expect(names).not.toContain('Ghost');
    expect(names).toHaveLength(3);
  });

  it('NEGATIVE: a count that did not resolve leaves rowCount undefined, not 0', () => {
    // 0 means "asked, and it is empty". undefined means "could not ask".
    // Collapsing them would recreate exactly the ambiguity this card is fixing.
    const out = annotateServed(clone(ATLAS), [{ kind: 'Product', collection: '/v1/products' }]);
    const product = out.domains.find((d) => d.name === 'products')?.classes[0];
    expect(product?.served).toBe(true);
    expect(product?.rowCount).toBeUndefined();
  });
});
