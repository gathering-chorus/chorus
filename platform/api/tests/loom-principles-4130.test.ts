// @test-type: unit
/**
 * #4130 — the loom serves the PC↔XP edges it holds.
 *
 * Measured 2026-09-09: chorus:rhymesWith was declared (principles-rhymes-4006.ttl,
 * 11 edges), stored (11 in urn:chorus:domains:principles), and served nowhere —
 * the handler hardcoded `parents: []` under a #3749 comment written before
 * #4006 authored the XP layer. Three suites were red on an edge that existed.
 *
 * The generator (athena-make lib.rs:944) puts an sh:class property on the
 * COLLECTION row as the target's local name — a bare string for one, string[]
 * for several, the same shape as ownedBy / hasDomain on /products. The entity
 * read serves literals only. So parents come from the collection row, and
 * labels resolve after the walk from the rows themselves.
 *
 * The states below are the point (#3734). A check that cannot separate a
 * served edge from an invented one is the defect, not the fix.
 */
import { fetchLoomPrinciples } from '../src/handlers/loom-principles';

type Row = Record<string, string>;
type CollectionRow = { name: string; rhymesWith?: string | string[] };

/** A fake athena-make: `rows` is the collection (edges ride here), `entities` what resolves. */
function fakeUpstream(rows: CollectionRow[], entities: Record<string, Row>) {
  return async (url: string | URL | Request): Promise<Response> => {
    const href = typeof url === 'string' ? url : url.toString();
    if (href.endsWith('/principles')) {
      return new Response(JSON.stringify({ data: rows }), { status: 200 });
    }
    const name = decodeURIComponent(href.split('/principles/')[1] ?? '');
    const row = entities[name];
    if (!row) return new Response(JSON.stringify({ kind: 'Error' }), { status: 404 });
    return new Response(JSON.stringify({ data: row }), { status: 200 });
  };
}

type Served = { id: string; label: string; parents: Array<{ id: string; label: string; uri: string }> };
const principles = (r: { body: unknown }) =>
  (r.body as { data: { principles: Served[] } }).data.principles;
const byId = (r: { body: unknown }, id: string) => principles(r).find((p) => p.id === id)!;

const PC = { 'hemenway-observe': { label: 'Observe', comment: 'c', isPermacultureParent: 'true' },
             'hemenway-connect': { label: 'Connect', comment: 'c', isPermacultureParent: 'true' } };

describe('#4130 loom principles — rhymesWith on the collection row becomes parents', () => {
  it('a single rhyme (bare string) resolves to one parent with its label and uri', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(
        [{ name: 'hemenway-observe' }, { name: 'xp-flow', rhymesWith: 'hemenway-observe' }],
        { ...PC, 'xp-flow': { label: 'Flow', comment: 'c', principleKind: 'xp' } },
      ) as unknown as typeof fetch,
    });
    expect(res.status).toBe(200);
    const flow = byId(res, 'xp-flow');
    expect(flow.parents).toHaveLength(1);
    expect(flow.parents[0]).toMatchObject({ id: 'hemenway-observe', label: 'Observe' });
    expect(flow.parents[0].uri).toContain('hemenway-observe');
  });

  it('several rhymes (string[]) resolve to several parents, in the order served', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(
        [{ name: 'hemenway-observe' }, { name: 'hemenway-connect' },
         { name: 'xp-diversity', rhymesWith: ['hemenway-connect', 'hemenway-observe'] }],
        { ...PC, 'xp-diversity': { label: 'Diversity', comment: 'c', principleKind: 'xp' } },
      ) as unknown as typeof fetch,
    });
    expect(byId(res, 'xp-diversity').parents.map((p) => p.id)).toEqual(['hemenway-connect', 'hemenway-observe']);
  });

  it('NEGATIVE PROOF: a rhyme to a principle that never walked is dropped, not invented', async () => {
    // The collection names a target that no entity read can resolve. The old
    // hardcoded [] would pass this by accident; a handler that fabricated a
    // parent from the bare name (no label, no walked row) would fail it.
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(
        [{ name: 'hemenway-observe' }, { name: 'xp-ghost', rhymesWith: ['hemenway-observe', 'hemenway-vanished'] }],
        { ...PC, 'xp-ghost': { label: 'Ghost', comment: 'c', principleKind: 'xp' } },
      ) as unknown as typeof fetch,
    });
    const ghost = byId(res, 'xp-ghost');
    expect(ghost.parents.map((p) => p.id)).toEqual(['hemenway-observe']);
    expect(ghost.parents.some((p) => p.id === 'hemenway-vanished')).toBe(false);
  });

  it('a row with no rhymesWith keeps parents: [] — the PC parents stay peers', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream([{ name: 'hemenway-observe' }], PC) as unknown as typeof fetch,
    });
    expect(byId(res, 'hemenway-observe').parents).toEqual([]);
  });
});
