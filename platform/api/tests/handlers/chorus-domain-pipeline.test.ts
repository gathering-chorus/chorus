// @test-type: unit — pure functions on fixtures; no live services
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
/**
 * chorus-domain-pipeline handler — unit tests (#2188, rewritten by #5044).
 *
 * #5044 — Jeff, on his phone, 2026-11-25 15:51: "pipeline section does not make sense to me".
 * The fold said "No cards" beside 16 cards and "Nothing shipped" above 5 releases. Each row now
 * reads what the fold beside it reads, and these fixtures are the real answer shapes of those
 * reads (taken from the live routes for principles on 2026-11-25), not shapes invented here:
 * the ship fixture this file used to carry held Done cards the old read never returned.
 */
import { fetchChorusDomainPipeline as pipeline, type ChorusDomainPipelineDeps } from '../../src/handlers/chorus-domain-pipeline';

const envelope = (queryName: string, data: unknown) => ({ _meta: { query_name: queryName }, data });

type Stage = { name: string; status: string; evidence: number; summary: string; detail: Record<string, unknown> };

// the reads the folds make, as the live routes answer them
const CARDS = '/api/chorus/domain/principles/cards';
const RELEASES = '/api/chorus/domain/principles/releases';
const TESTS = '/api/chorus/domain/principles/tests';
const ALERTS = '/api/chorus/domain/principles/alerts';
const FILES = '/owl/code/files?hasDomain=principles&limit=5000';
// #5042 — routes are read from the domain's OpenAPI documents, as the model door's discovery names them
const DISCOVERY = '/owl/';
const DOC = '/owl/v1/principles/principles/openapi.json';
const LOOM_DOC = '/owl/v1/loom/pages/openapi.json';
const COMPLETENESS = '/owl/domains/domains/principles/completeness';

const card = (id: string, status: string) => ({ id, title: `card ${id}`, owner: 'wren', status, priority: 'P2' });
const row = (n: number, domain: string | string[] = 'principles') => ({ name: `row-${n}`, hasDomain: domain });

function live(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    [CARDS]: { domain: 'principles', cards: [card('5033', 'Done'), card('5021', 'Done'), card('5017', 'Done')] },
    [RELEASES]: { _meta: { count: 2 }, data: { domain: 'principles', releases: [
      { cardId: '5033', timestamp: '2026-11-24T11:42:21-05:00', title: 'a' },
      { cardId: '5021', timestamp: '2026-11-21T10:18:00-05:00', title: 'b' },
    ] } },
    [TESTS]: { _meta: { count: 26 }, data: { domain: 'principles', tests: 26, total: 175 } },
    [ALERTS]: { _meta: { count: 3 }, data: { alerts: [{}, {}, {}] } },
    [FILES]: { data: [row(1), row(2), row(3, ['principles', 'loom'])] },
    [DISCOVERY]: { kind: 'Discovery', primitives: [
      { kind: 'Principle', collection: '/v1/principles/principles', openapi: '/v1/principles/principles/openapi.json' },
      { kind: 'Page', collection: '/v1/loom/pages', openapi: '/v1/loom/pages/openapi.json' },
    ] },
    [DOC]: { openapi: '3.1.0', paths: { '/principles/principles': { get: {} }, '/principles/principles/{name}': { get: {} } } },
    [LOOM_DOC]: { openapi: '3.1.0', paths: { '/loom/pages': { get: {}, post: {} } } },
    ...over,
  };
}

function reads(routes: Record<string, unknown>, completeness: unknown = { percentage: 100, present: ['label'], missing: [] }): ChorusDomainPipelineDeps & { asked: string[] } {
  const asked: string[] = [];
  return {
    asked,
    fetcher: async (url: string) => {
      asked.push(url);
      if (url === COMPLETENESS) return completeness;
      return url in routes ? routes[url] : null;
    },
    envelope,
    now: () => 1_000_000,
  };
}

function byName(result: { body: unknown }): Record<string, Stage> {
  return Object.fromEntries((result.body as { data: { stages: Stage[] } }).data.stages.map((s) => [s.name, s]));
}

describe('fetchChorusDomainPipeline (#5044: each row reads what the fold beside it reads)', () => {
  test('shape counts the cards the Cards fold lists, Done ones included', async () => {
    const s = byName(await pipeline(reads(live()), 'principles'));
    expect(s.shape.evidence).toBe(3);
    expect(s.shape.summary).toBe('3 cards (0 open, 3 done)');
    expect(s.shape.status).toBe('in progress');
  });

  test('ship counts the releases the Releases fold lists, and names the newest', async () => {
    const s = byName(await pipeline(reads(live()), 'principles'));
    expect(s.ship.evidence).toBe(2);
    expect(s.ship.status).toBe('complete');
    expect(s.ship.summary).toBe('2 releases, newest #5033');
  });

  test('NEGATIVE: Done cards with no release do not read as shipped, and no release reads "not started"', async () => {
    const s = byName(await pipeline(reads(live({ [RELEASES]: { data: { releases: [] } } })), 'principles'));
    expect(s.ship.evidence).toBe(0);
    expect(s.ship.status).toBe('not started');
    expect(s.ship.summary).toBe('No releases');
  });

  test('build counts the files and routes the Code and API Contract folds list', async () => {
    const s = byName(await pipeline(reads(live()), 'principles'));
    expect(s.build.detail).toEqual({ code: 3, tests: 26, endpoints: 2 });
    expect(s.build.summary).toBe('3 files, 26 test files, 2 routes');
    expect(s.build.status).toBe('complete');
  });

  test('NEGATIVE: another domain\'s OpenAPI document is not counted, and an unreadable document leaves the row unread', async () => {
    const d = reads(live());
    const s = byName(await pipeline(d, 'principles'));
    expect(d.asked).not.toContain(LOOM_DOC);
    expect(s.build.detail.endpoints).toBe(2);
    const broken = byName(await pipeline(reads(live({ [DOC]: null })), 'principles'));
    expect(broken.build.summary).toBe('could not read routes');
  });

  test('NEGATIVE: a row of another domain that the door sent anyway is not counted', async () => {
    const s = byName(await pipeline(reads(live({ [FILES]: { data: [row(1), row(2, 'loom'), row(3, ['loom', 'cards'])] } })), 'principles'));
    expect(s.build.detail.code).toBe(1);
  });

  test('design reads completeness under the domain\'s own name, never a made-up "-service" name', async () => {
    const d = reads(live(), { percentage: 100, present: ['comment', 'gaps', 'label', 'purpose', 'status'], missing: [] });
    const s = byName(await pipeline(d, 'principles'));
    expect(d.asked.filter((u) => u.includes('completeness'))).toEqual([COMPLETENESS]);
    expect(s.design.status).toBe('complete');
    expect(s.design.summary).toBe('100%: 5 present, 0 missing');
  });

  test('design: 40% is in progress, 0% is not started', async () => {
    const forty = byName(await pipeline(reads(live(), { percentage: 40, present: ['a', 'b'], missing: ['c', 'd', 'e'] }), 'principles'));
    const zero = byName(await pipeline(reads(live(), { percentage: 0, present: [], missing: ['a'] }), 'principles'));
    expect(forty.design.status).toBe('in progress');
    expect(zero.design.status).toBe('not started');
  });

  test('prove counts the alert rules the Alerts fold lists', async () => {
    const s = byName(await pipeline(reads(live()), 'principles'));
    const none = byName(await pipeline(reads(live({ [ALERTS]: { _meta: { count: 0 }, data: { alerts: [] } } })), 'principles'));
    expect(s.prove.status).toBe('complete');
    expect(s.prove.summary).toBe('3 alert rules');
    expect(none.prove.status).toBe('not started');
  });

  // The old file's first test held the fault in place: "all fetches return null → all stages
  // not_started". A read that fails is not a domain that has not started.
  test('NEGATIVE: a read that fails is shown as failed, never as "not started"', async () => {
    const s = byName(await pipeline(reads({}, null), 'principles'));
    for (const name of ['shape', 'design', 'build', 'prove', 'ship']) {
      expect(s[name].status).toBe('could not read');
      expect(s[name].summary).toMatch(/^could not read /);
    }
  });

  test('NEGATIVE: one failed read fails its own row only, and says which read', async () => {
    const routes = live();
    delete routes[RELEASES];
    const s = byName(await pipeline(reads(routes), 'principles'));
    expect(s.ship.status).toBe('could not read');
    expect(s.ship.summary).toBe('could not read releases');
    expect(s.shape.status).toBe('in progress');
    expect(s.build.status).toBe('complete');
  });

  test('NEGATIVE: build with one of its three reads failed does not print a count for it', async () => {
    const routes = live();
    delete routes[DISCOVERY];
    const s = byName(await pipeline(reads(routes), 'principles'));
    expect(s.build.status).toBe('could not read');
    expect(s.build.summary).toBe('could not read routes');
  });

  test('never asks the old domain read (25 fixed names) or a retired /api/athena/domains route', async () => {
    const d = reads(live());
    await pipeline(d, 'principles');
    expect(d.asked).not.toContain('/api/chorus/domain/principles');
    expect(d.asked.filter((u) => u.includes('/api/athena/domains'))).toEqual([]);
    expect(d.asked).toEqual(expect.arrayContaining([CARDS, RELEASES, TESTS, ALERTS, FILES, DISCOVERY, DOC]));
  });

  test('statuses are plain words: no underscore reaches the page', async () => {
    const all = [
      byName(await pipeline(reads(live()), 'principles')),
      byName(await pipeline(reads({}, null), 'principles')),
      byName(await pipeline(reads(live({ [RELEASES]: { data: { releases: [] } } }), { percentage: 40, present: [], missing: [] }), 'principles')),
    ];
    for (const s of all) for (const st of Object.values(s)) expect(st.status).not.toMatch(/_/);
  });

  test('envelope wraps body with query_name "domain-pipeline", five stages in order', async () => {
    const body = (await pipeline(reads(live()), 'principles')).body as {
      _meta: { query_name: string }; data: { stages: Stage[] };
    };
    expect(body._meta.query_name).toBe('domain-pipeline');
    expect(body.data.stages.map((s) => s.name)).toEqual(['shape', 'design', 'build', 'prove', 'ship']);
  });
});
