// @test-type: unit — pure guards; no server boot, no Fuseki, no network
//
// #4004 — Jeff's phone photo: the ontology ER page renders its frame then
// "Failed to load". The page hardcoded http://localhost:3030/pods/sparql, and on
// a phone `localhost` IS the phone — it worked on the Mac and nowhere else, the
// public share included. The same-origin proxy is the fix; these pin that it
// stays a READ door, because a query proxy that would forward an update is a
// write surface wearing a read name.
import { isReadOnlySparql, sparqlDataset } from '../src/server';

describe('#4004 sparql-read guards', () => {
  it('NEGATIVE PROOF: every update verb is refused', () => {
    for (const q of [
      'INSERT DATA { <a> <b> <c> }',
      'DELETE WHERE { ?s ?p ?o }',
      'DROP GRAPH <g>',
      'CLEAR ALL',
      'LOAD <http://x/>',
      'copy graph <a> to <b>',
    ]) {
      expect(isReadOnlySparql(q)).toBe(false);
    }
  });

  it('NEGATIVE PROOF: read queries pass — the guard separates its two states', () => {
    for (const q of ['ASK {}', 'SELECT * WHERE { ?s ?p ?o }', 'CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }']) {
      expect(isReadOnlySparql(q)).toBe(true);
    }
  });

  it('the dataset name cannot escape into another endpoint', () => {
    expect(sparqlDataset('../admin')).toBe('admin');
    expect(sparqlDataset('pods')).toBe('pods');
    expect(sparqlDataset(undefined)).toBe('pods');
    expect(sparqlDataset('///')).toBe('pods');
  });
});
