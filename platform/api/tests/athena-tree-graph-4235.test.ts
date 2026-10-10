// @test-type: unit
// @domain: tests · @card: 4235 · owner: kade
// #4235 — the Athena tree and ownership lookups answer from the live graph.
// Jeff, 2026-10-10: "i honestly dont think we want any tree.json dependencies".
import { treeFromCsv, lookupOwnership, loadTreeFromGraph, structureQuery } from '../src/handlers/athena-tree-graph';

const C = 'https://jeffbridwell.com/chorus#';
const RDFS = 'http://www.w3.org/2000/01/rdf-schema#';
const row = (s: string, k: string, p: string, o: string) => `${C}${s},${C}${k},${p},${o}`;
const CSV = [
  's,k,p,o',
  row('chorus', 'Product', `${RDFS}label`, 'Chorus'),
  row('clearing', 'Product', `${RDFS}label`, 'The Clearing'),
  row('clearing', 'Product', `${C}partOf`, `${C}chorus`),
  row('clearing', 'Product', `${C}ownedBy`, `${C}principal-wren`),
  row('clearing', 'Product', `${C}hasDomain`, `${C}messages`),
  row('messages', 'Domain', `${RDFS}comment`, '"messages, threads and rooms"'),
  row('messages', 'Domain', `${C}ownedBy`, `${C}principal-wren`),
  row('messages', 'Domain', `${C}hosts`, `${C}service-pulse`),
  row('service-pulse', 'Service', `${C}ownedBy`, `${C}principal-kade`),
  `${C}identity,${C}Domain,,`,
].join('\n');

describe('#4235 the tree is built from the graph', () => {
  it('products, domains and services come from the rows, children from partOf', () => {
    const tree = treeFromCsv(CSV);
    expect(tree.source).toBe('graph');
    expect(tree.products.map((p) => p.iri)).toEqual(['chorus:chorus', 'chorus:clearing']);
    expect(tree.products.find((p) => p.iri === 'chorus:chorus')!.hasChild).toEqual(['chorus:clearing']);
    expect(tree.domains.map((d) => d.iri)).toEqual(['chorus:identity', 'chorus:messages']);
    expect(tree.domains.find((d) => d.iri === 'chorus:messages')!.comment).toBe('messages, threads and rooms');
    expect(tree.services[0].iri).toBe('chorus:service-pulse');
  });

  it('a domain with no fields yet is still in the tree, named by its IRI', () => {
    expect(treeFromCsv(CSV).domains.find((d) => d.iri === 'chorus:identity')!.label).toBe('identity');
  });

  it('ownership: a domain names its product, a service its domain and product', () => {
    const tree = treeFromCsv(CSV);
    expect(lookupOwnership(tree, 'chorus:messages')).toEqual({
      iri: 'chorus:messages', kind: 'domain', owner: 'chorus:principal-wren', product: 'chorus:clearing', domain: 'chorus:messages' });
    expect(lookupOwnership(tree, 'chorus:service-pulse')).toEqual({
      iri: 'chorus:service-pulse', kind: 'service', owner: 'chorus:principal-kade',
      product: 'chorus:clearing', domain: 'chorus:messages', service: 'chorus:service-pulse' });
  });

  it('NEGATIVE PROOF — a name the graph does not hold is not-found, and a store that does not answer is null', async () => {
    expect(lookupOwnership(treeFromCsv(CSV), 'chorus:nowhere')).toBeNull();
    expect(await loadTreeFromGraph(async () => null)).toBeNull();
    const added = treeFromCsv(`${CSV}\n${row('borg', 'Product', `${RDFS}label`, 'Borg')}`);
    expect(added.products.map((p) => p.iri)).toContain('chorus:borg');
  });

  it('the query keeps subjects with no fields', () => {
    expect(structureQuery()).toContain('OPTIONAL {');
  });
});
