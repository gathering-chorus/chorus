// @test-type: unit
// @domain: tests · @card: 4419 · owner: kade
// #4419 AC1 — blast radius answers from the live graph's edges, not the June
// tree.json. The Clearing names the domains it is made of as dependents.
import { blastRadiusFromEdges, type Edge } from '../src/handlers/athena-blast-radius-graph';

const C = 'https://jeffbridwell.com/chorus#';
const e = (s: string, k: string, p: string, o: string): Edge => ({ s: C + s, kind: k, p, o: C + o });
const EDGES: Edge[] = [
  e('clearing', 'Product', 'hasDomain', 'cards'),
  e('clearing', 'Product', 'hasDomain', 'messages'),
  e('clearing', 'Product', 'hasDomain', 'streams'),
  e('clearing', 'Product', 'partOf', 'chorus'),
  e('chorus', 'Product', 'hasDomain', 'domains'),
  e('borg', 'Product', 'consumes', 'messages'),
  e('messages', 'Domain', 'hosts', 'pulse-service'),
  e('athena', 'Product', 'consumes', 'pulse-service'),
];

describe('#4419 blast radius from the graph', () => {
  it('a product names its domains as dependents and its parent as a consumer', () => {
    const r = blastRadiusFromEdges(EDGES, 'chorus:clearing')!;
    expect(r.dependents.sort()).toEqual(['chorus:cards', 'chorus:messages', 'chorus:streams']);
    expect(r.consumers).toContain('chorus:chorus');
  });

  it('a domain names the products made of it, the products consuming it, and the consumers of the services it hosts', () => {
    const r = blastRadiusFromEdges(EDGES, 'chorus:messages')!;
    expect(r.consumers.sort()).toEqual(['chorus:athena', 'chorus:borg', 'chorus:clearing']);
    expect(r.hosts).toEqual(['chorus:pulse-service']);
  });

  it('NEGATIVE PROOF — an edge added to the graph changes the answer with no file edit', () => {
    const before = blastRadiusFromEdges(EDGES, 'chorus:clearing')!;
    const after = blastRadiusFromEdges([...EDGES, e('clearing', 'Product', 'hasDomain', 'pulse')], 'chorus:clearing')!;
    expect(before.dependents).not.toContain('chorus:pulse');
    expect(after.dependents).toContain('chorus:pulse');
  });

  it('an IRI the graph does not hold is not-found, never an empty answer', () => {
    expect(blastRadiusFromEdges(EDGES, 'chorus:nowhere')).toBeNull();
  });
});
