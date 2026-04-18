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
    ).body as { data: { subdomain: string; releases: unknown[] } };
    expect(body.data.releases.length).toBe(1);
    expect(body.data.subdomain).toBe('photos-domain'); // preserved
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
