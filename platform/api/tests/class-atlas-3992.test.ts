// @test-type: unit
// #3992 — Class Atlas: buildClassAtlas turns one SPARQL result set into the
// per-domain UML view (classes + attributes + multiplicities + object edges +
// subclass edges, cross-domain flagged). The fixture rows are CAPTURED from a
// live query against urn:chorus:ontology on 2026-08-27 — never hand-shaped
// (hand-typed fixtures prove nothing about real bindings).
import { buildClassAtlas, SparqlBinding } from '../src/handlers/class-atlas';

const CH = 'https://jeffbridwell.com/chorus#';
const XSD = 'http://www.w3.org/2001/XMLSchema#';
const uri = (l: string) => ({ type: 'uri', value: CH + l });
const int = (v: string) => ({ type: 'literal', value: v, datatype: XSD + 'integer' });
const xsd = (l: string) => ({ type: 'uri', value: XSD + l });

// Captured rows: board.ChunkMembership (object props with min only),
// board.Chunk (loomSequence has NO minCount → optional), services.Service
// (bnode inverse-path row with min=1 max=1 → must be labelled, not dropped;
// hasDesignDoc → cross-domain edge to documents.Document),
// tests.Test (subClassOf Hydratable).
const ROWS = [
  { domain: uri('board'), class: uri('ChunkMembership'), prop: uri('inChunk'), min: int('1'), rc: uri('Chunk') },
  { domain: uri('board'), class: uri('ChunkMembership'), prop: uri('hasCard'), min: int('1'), rc: uri('Card') },
  { domain: uri('board'), class: uri('ChunkMembership'), prop: uri('rank'), min: int('1'), dt: xsd('integer') },
  { domain: uri('board'), class: uri('Chunk'), prop: uri('label'), min: int('1'), dt: xsd('string') },
  { domain: uri('board'), class: uri('Chunk'), prop: uri('loomSequence'), dt: xsd('integer') },
  { domain: uri('services'), class: uri('Service'), prop: { type: 'bnode', value: 'b0' }, min: int('1'), max: int('1'), rc: uri('Domain') },
  { domain: uri('services'), class: uri('Service'), prop: uri('hasDesignDoc'), min: int('1'), rc: uri('Document') },
  { domain: uri('services'), class: uri('Service'), prop: uri('label'), min: int('1'), dt: xsd('string') },
  { domain: uri('tests'), class: uri('Test'), prop: uri('inFile'), min: int('1'), rc: uri('SourceFile'), parent: uri('Hydratable') },
] as unknown as SparqlBinding[];

// class→home-domain map the handler derives from definesVocabulary; the atlas
// needs it to mark an edge cross-domain (Document lives in documents, not services).
const HOMES = new Map(Object.entries({
  Chunk: 'board', ChunkMembership: 'board', Card: 'cards', Service: 'services',
  Domain: 'domains', Document: 'documents', Test: 'tests', SourceFile: 'code', Hydratable: 'code',
}));

const domain = (name: string) => {
  const d = buildClassAtlas(ROWS, HOMES).domains.find((x) => x.name === name);
  if (!d) throw new Error(`domain ${name} missing from atlas`);
  return d;
};

describe('buildClassAtlas (#3992)', () => {
  it('groups classes under their domains with attributes typed and required-marked', () => {
    const chunk = domain('board').classes.find((c) => c.name === 'Chunk');
    expect(chunk!.attributes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'label', type: 'string', min: 1 }),
        expect.objectContaining({ name: 'loomSequence', type: 'integer', min: 0 }),
      ])
    );
  });

  it('renders SHACL min..max as multiplicities, unbounded max as *', () => {
    const cm = domain('board').classes.find((c) => c.name === 'ChunkMembership')!;
    expect(cm.edges.find((e) => e.name === 'inChunk')!.multiplicity).toBe('1..*');
    const svc = domain('services').classes.find((c) => c.name === 'Service')!;
    expect(svc.edges.find((e) => e.name === '(inverse path)')!.multiplicity).toBe('1..1');
  });

  it('flags cross-domain edges and keeps in-domain edges unflagged', () => {
    const svc = domain('services').classes.find((c) => c.name === 'Service')!;
    expect(svc.edges.find((e) => e.name === 'hasDesignDoc')!.crossDomain).toBe(true);
    const cm = domain('board').classes.find((c) => c.name === 'ChunkMembership')!;
    expect(cm.edges.find((e) => e.name === 'inChunk')!.crossDomain).toBe(false);
  });

  it('carries subclass edges', () => {
    expect(domain('tests').classes.find((c) => c.name === 'Test')!.parents).toEqual(['Hydratable']);
  });

  it('never invents content: empty bindings yield empty domains, not defaults', () => {
    expect(buildClassAtlas([] as SparqlBinding[], new Map()).domains).toEqual([]);
  });
});
