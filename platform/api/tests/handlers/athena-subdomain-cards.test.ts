/**
 * athena-subdomain-cards handler — unit tests (#2187).
 *
 * Maps a sub-domain ID to board-card search terms, filters cards by
 * domain:<label> or sequence:<label> tag match. Aliases expand search
 * labels for aggregation domains (e.g., tests → quality, code → code).
 */
import {
  fetchAthenaSubdomainCards,
  type AthenaSubdomainCardsDeps,
  type BoardCard,
} from '../../src/handlers/athena-subdomain-cards';

function card(over: Partial<BoardCard> = {}): BoardCard {
  return {
    id: '1', title: 't', owner: 'silas', status: 'WIP', priority: 'P1', tags: '',
    ...over,
  };
}

function deps(overrides: Partial<AthenaSubdomainCardsDeps> = {}): AthenaSubdomainCardsDeps {
  return {
    getBoardCards: () => [],
    now: () => 1_000_000,
    ...overrides,
  };
}

describe('fetchAthenaSubdomainCards (#2187)', () => {
  test('empty board returns 200 with empty cards list', async () => {
    const r = await fetchAthenaSubdomainCards(deps(), 'chorus-domain');
    expect(r.status).toBe(200);
    const body = r.body as { data: { subdomain: string; domainLabel: string; cards: Array<unknown> } };
    expect(body.data.subdomain).toBe('chorus-domain');
    expect(body.data.domainLabel).toBe('chorus');
    expect(body.data.cards).toEqual([]);
  });

  test('strips -domain suffix only; -service / -analytics words kept (#2430)', async () => {
    // Pre-#2430 this handler over-aggressively stripped -service and -analytics,
    // silently collapsing real subdomain ids (e.g. loom-analytics → loom).
    // Resolver-strict contract: only -domain is a namespace suffix.
    const r1 = await fetchAthenaSubdomainCards(deps(), 'chorus-domain');
    expect((r1.body as { data: { domainLabel: string } }).data.domainLabel).toBe('chorus');

    const r2 = await fetchAthenaSubdomainCards(deps(), 'photos-service');
    expect((r2.body as { data: { domainLabel: string } }).data.domainLabel).toBe('photos-service');

    const r3 = await fetchAthenaSubdomainCards(deps(), 'music-analytics');
    expect((r3.body as { data: { domainLabel: string } }).data.domainLabel).toBe('music-analytics');
  });

  test('card with matching domain:<label> tag is included', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => [
        card({ id: '100', title: 'A', tags: 'domain:chorus' }),
        card({ id: '200', title: 'B', tags: 'domain:photos' }),
      ],
    }), 'chorus-domain');
    const body = r.body as { data: { cards: Array<{ id: string }> } };
    expect(body.data.cards.map((c) => c.id)).toEqual(['100']);
  });

  test('card with matching sequence:<label> tag is included', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => [
        card({ id: '300', title: 'S', tags: 'sequence:chorus' }),
      ],
    }), 'chorus-domain');
    const body = r.body as { data: { cards: Array<{ id: string }> } };
    expect(body.data.cards.map((c) => c.id)).toEqual(['300']);
  });

  test('tests alias expands to also match quality tag', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => [
        card({ id: '500', tags: 'domain:quality' }),
        card({ id: '600', tags: 'domain:tests' }),
      ],
    }), 'tests-domain');
    const body = r.body as { data: { cards: Array<{ id: string }> } };
    expect(body.data.cards.map((c) => c.id).sort()).toEqual(['500', '600']);
  });

  test('card response shape has only id, title, owner, status, priority', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => [
        card({ id: '1', title: 'T', owner: 'wren', status: 'Next', priority: 'P2', tags: 'domain:chorus,extra:stuff' }),
      ],
    }), 'chorus-domain');
    const body = r.body as { data: { cards: Array<Record<string, unknown>> } };
    expect(body.data.cards[0]).toEqual({ id: '1', title: 'T', owner: 'wren', status: 'Next', priority: 'P2' });
  });

  test('getBoardCards throwing maps to 500 with error envelope', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => { throw new Error('cache miss'); },
    }), 'x');
    expect(r.status).toBe(500);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('cache miss');
    expect(body._meta.error).toBe(true);
  });

  test('count in meta matches cards length', async () => {
    const r = await fetchAthenaSubdomainCards(deps({
      getBoardCards: () => [
        card({ tags: 'domain:x' }),
        card({ tags: 'domain:x' }),
        card({ tags: 'domain:y' }),
      ],
    }), 'x');
    const body = r.body as { data: { cards: Array<unknown> }; _meta: { count: number } };
    expect(body._meta.count).toBe(2);
    expect(body.data.cards).toHaveLength(2);
  });
});
