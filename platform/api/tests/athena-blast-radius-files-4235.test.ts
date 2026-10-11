// @test-type: unit
// @domain: tests · @card: 4235 · owner: kade
// #4235 — blast radius answers for every live domain and names the files the
// crawl tagged with it. Jeff, 2026-09-20: "u do understand why i want them to
// use crawler data right?"
import {
  blastRadiusFromEdges, edgesFromCsv, fileTagsFromCsv, blastRadiusEdgesQuery, type Edge, type FileTag,
} from '../src/handlers/athena-blast-radius-graph';

const C = 'https://jeffbridwell.com/chorus#';
const e = (s: string, k: string, p: string, o: string): Edge => ({ s: C + s, kind: k, p, o: C + o });
const EDGES: Edge[] = [
  e('clearing', 'Product', 'hasDomain', 'messages'),
  e('clearing', 'Product', 'hasDomain', 'cards'),
  e('messages', 'Domain', 'hosts', 'pulse-service'),
];
const FILES: FileTag[] = [
  { domain: C + 'messages', path: 'platform/services/pulse/src/main.rs' },
  { domain: C + 'messages', path: 'directing/clearing/src/messages.ts' },
  { domain: C + 'cards', path: 'platform/scripts/cards' },
  { domain: C + 'identity', path: 'platform/services/chorus-principal/src/main.rs' },
];

describe('#4235 blast radius names files from the crawl', () => {
  it('a domain names the files tagged with it', () => {
    const r = blastRadiusFromEdges(EDGES, 'chorus:messages', FILES)!;
    expect(r.files).toEqual(['directing/clearing/src/messages.ts', 'platform/services/pulse/src/main.rs']);
  });

  it("a product names its domains' files", () => {
    const r = blastRadiusFromEdges(EDGES, 'chorus:clearing', FILES)!;
    expect(r.files).toEqual(['directing/clearing/src/messages.ts', 'platform/scripts/cards', 'platform/services/pulse/src/main.rs']);
  });

  it('a domain with no edges yet answers, with its files — not not-found (the identity case)', () => {
    const csv = `s,k,p,o\n${C}identity,${C}Domain,,\n${C}clearing,${C}Product,${C}hasDomain,${C}messages\n`;
    const r = blastRadiusFromEdges(edgesFromCsv(csv), 'chorus:identity', FILES)!;
    expect(r).not.toBeNull();
    expect(r.consumers).toEqual([]);
    expect(r.files).toEqual(['platform/services/chorus-principal/src/main.rs']);
  });

  it('NEGATIVE PROOF — a file tag added by the crawl changes the answer with no file edit', () => {
    const before = blastRadiusFromEdges(EDGES, 'chorus:cards', FILES)!;
    const after = blastRadiusFromEdges(EDGES, 'chorus:cards', [...FILES, { domain: C + 'cards', path: 'platform/api/src/cards.ts' }])!;
    expect(before.files).not.toContain('platform/api/src/cards.ts');
    expect(after.files).toContain('platform/api/src/cards.ts');
  });

  it('a name the graph does not hold is still not-found', () => {
    expect(blastRadiusFromEdges(EDGES, 'chorus:nowhere', FILES)).toBeNull();
  });

  it('the query keeps edge-less subjects, and file tags parse from CSV', () => {
    expect(blastRadiusEdgesQuery()).toContain('OPTIONAL {');
    expect(fileTagsFromCsv(`d,path\n${C}cards,platform/scripts/cards\n${C}x,\n`))
      .toEqual([{ domain: C + 'cards', path: 'platform/scripts/cards' }]);
  });
});
