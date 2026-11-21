// @test-type: unit — stubbed dependencies; no live service.
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
/**
 * chorus-domain-releases handler — unit tests (#2188).
 */
import { fetchChorusDomainReleases, type ChorusDomainReleasesDeps, type ReleasesBoardCard } from '../../src/handlers/chorus-domain-releases';

const envelope = (queryName: string, data: unknown, _d: number, extra?: Record<string, unknown>) => ({
  _meta: { query_name: queryName, ...extra }, data,
});

function deps(over: Partial<ChorusDomainReleasesDeps> = {}): ChorusDomainReleasesDeps {
  return {
    gitLog: () => '',
    getCards: () => [],
    envelope,
    now: () => 1_000_000,
    ...over,
  };
}

describe('fetchChorusDomainReleases (#2188)', () => {
  test('empty git log → empty releases', () => {
    const body = fetchChorusDomainReleases(deps(), 'photos').body as {
      _meta: { count: number; total_acps: number }; data: { releases: unknown[] };
    };
    expect(body._meta.count).toBe(0);
    expect(body._meta.total_acps).toBe(0);
    expect(body.data.releases).toEqual([]);
  });

  test('ACP matching card domain tag → gates=passed', () => {
    const log = 'abc12345|2026-04-18T10:00:00Z|wren: acp #42 — photos thumb pipeline';
    const cards: ReleasesBoardCard[] = [
      { id: '42', tags: 'domain:photos type:enhance' },
    ];
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log, getCards: () => cards }),
      'photos',
    ).body as { data: { releases: Array<{ gates: string; cardId: string; commit: string; title: string }> } };
    expect(body.data.releases.length).toBe(1);
    expect(body.data.releases[0].gates).toBe('passed');
    expect(body.data.releases[0].cardId).toBe('42');
    expect(body.data.releases[0].commit).toBe('abc12345');
  });

  test('ACP with no card entry but title contains domain → gates=unknown', () => {
    const log = 'abc12345|2026-04-18T10:00:00Z|silas: acp #99 — photos pipeline fix';
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log, getCards: () => [] }),
      'photos',
    ).body as { data: { releases: Array<{ gates: string }> } };
    expect(body.data.releases[0].gates).toBe('unknown');
  });

  test('ACP with card in wrong domain → excluded', () => {
    const log = 'abc12345|2026-04-18T10:00:00Z|wren: acp #42 — music thing';
    const cards: ReleasesBoardCard[] = [{ id: '42', tags: 'domain:music' }];
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log, getCards: () => cards }),
      'photos',
    ).body as { data: { releases: unknown[] } };
    expect(body.data.releases).toEqual([]);
  });

  test('sequence: tag also counts as domain match', () => {
    const log = 'abc12345|2026-04-18T10:00:00Z|kade: acp #7 — quality gate adjustment';
    const cards: ReleasesBoardCard[] = [{ id: '7', tags: 'sequence:photos' }];
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log, getCards: () => cards }),
      'photos',
    ).body as { data: { releases: Array<{ gates: string }> } };
    expect(body.data.releases[0].gates).toBe('passed');
  });

  test('domain name strips -domain/-service/-analytics suffix', () => {
    const log = 'abc12345|2026-04-18T10:00:00Z|wren: acp #42 — photos pipeline';
    const cards: ReleasesBoardCard[] = [{ id: '42', tags: 'domain:photos' }];
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log, getCards: () => cards }),
      'photos-domain',
    ).body as { data: { domain: string; releases: unknown[] } };
    expect(body.data.releases.length).toBe(1);
    expect(body.data.domain).toBe('photos-domain'); // preserved
  });

  test('non-ACP lines ignored', () => {
    const log = [
      'abc12345|2026-04-18T10:00:00Z|wren: regular commit msg',
      'def67890|2026-04-18T11:00:00Z|wren: #42 extract handlers',
      '999ffff0|2026-04-18T12:00:00Z|silas: acp #99 — photos pipeline',
    ].join('\n');
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => log }),
      'photos',
    ).body as { _meta: { total_acps: number }; data: { releases: unknown[] } };
    expect(body._meta.total_acps).toBe(1);
    expect(body.data.releases.length).toBe(1);
  });

  test('gitLog throw → empty envelope', () => {
    const body = fetchChorusDomainReleases(
      deps({ gitLog: () => { throw new Error('git fail'); } }),
      'photos',
    ).body as { _meta: { count: number }; data: { releases: unknown[] } };
    expect(body._meta.count).toBe(0);
    expect(body.data.releases).toEqual([]);
  });

  test('envelope carries query_name "domain-releases"', () => {
    const body = fetchChorusDomainReleases(deps(), 'photos').body as {
      _meta: { query_name: string };
    };
    expect(body._meta.query_name).toBe('domain-releases');
  });
});

// #5021 — a land since #4707 is "#<card> (<role>) (#<PR>)"; the acp-only reader found none of them
describe('#5021 land commits are releases', () => {
  const log = [
    'aaaa1111|2026-11-20T19:32:54Z|#5017 (kade) (#2007)',
    'bbbb2222|2026-11-20T22:40:00Z|#5020 (kade) (#2008)',
    'cccc3333|2026-11-20T23:00:00Z|silas: #5018 — silas: #5018 AC4 — a werk commit, not a land (#2006)',
  ].join('\n');
  const cards: ReleasesBoardCard[] = [
    { id: '5017', tags: 'Kade|P2|domain:principles|type:enhance', title: 'principles get their own door' },
    { id: '5020', tags: 'Kade|P2|domain:chorus|type:enhance', title: 'domain lint' },
  ];
  test('a land of a principles card is a principles release, titled from the board', () => {
    const body = fetchChorusDomainReleases(deps({ gitLog: () => log, getCards: () => cards }), 'principles').body as {
      data: { releases: Array<{ cardId: string; role: string; title: string; gates: string }> };
    };
    expect(body.data.releases).toEqual([expect.objectContaining({ cardId: '5017', role: 'kade', title: 'principles get their own door', gates: 'passed' })]);
  });
  test('NEGATIVE: another domain\'s land and a werk commit are not principles releases', () => {
    const body = fetchChorusDomainReleases(deps({ gitLog: () => log, getCards: () => cards }), 'principles').body as {
      data: { releases: Array<{ cardId: string }> };
    };
    expect(body.data.releases.map((r) => r.cardId)).not.toContain('5020');
    expect(body.data.releases.map((r) => r.cardId)).not.toContain('5018');
  });
});
