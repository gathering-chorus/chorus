import { fetchAthenaCardDetail, type AthenaCardDetailDeps } from '../../src/handlers/athena-card-detail';

const deps = (over: Partial<AthenaCardDetailDeps> = {}): AthenaCardDetailDeps => ({
  runCardsView: async () => JSON.stringify({ id: '1', title: 'T' }),
  now: () => 1_000_000,
  ...over,
});

describe('fetchAthenaCardDetail (#2187)', () => {
  test('parses JSON card and returns 200 with envelope', async () => {
    const r = await fetchAthenaCardDetail(deps({
      runCardsView: async () => JSON.stringify({ id: '42', title: 'Widget', status: 'WIP' }),
    }), '42');
    expect(r.status).toBe(200);
    const body = r.body as { data: { id: string; title: string; status: string; ac_items: Array<unknown> } };
    expect(body.data.id).toBe('42');
    expect(body.data.ac_items).toEqual([]);
  });

  test('extracts unchecked and checked AC items from description', async () => {
    const desc = 'Preamble text\n\n## AC\n- [ ] First item\n- [x] Second item done\n- [ ] Third item';
    const r = await fetchAthenaCardDetail(deps({
      runCardsView: async () => JSON.stringify({ id: '1', description: desc }),
    }), '1');
    const body = r.body as { data: { ac_items: Array<{ text: string; checked: boolean }> } };
    expect(body.data.ac_items).toEqual([
      { text: 'First item', checked: false },
      { text: 'Second item done', checked: true },
      { text: 'Third item', checked: false },
    ]);
  });

  test('missing description yields empty ac_items', async () => {
    const r = await fetchAthenaCardDetail(deps({
      runCardsView: async () => JSON.stringify({ id: '1' }),
    }), '1');
    const body = r.body as { data: { ac_items: Array<unknown> } };
    expect(body.data.ac_items).toEqual([]);
  });

  test('cards CLI throws → 404 with error envelope', async () => {
    const r = await fetchAthenaCardDetail(deps({
      runCardsView: async () => { throw new Error('card not found'); },
    }), 'missing');
    expect(r.status).toBe(404);
    const body = r.body as { data: { error: string }; _meta: { error: boolean } };
    expect(body.data.error).toBe('Card missing not found');
    expect(body._meta.error).toBe(true);
  });

  test('invalid JSON from CLI → 404', async () => {
    const r = await fetchAthenaCardDetail(deps({
      runCardsView: async () => 'not json',
    }), '1');
    expect(r.status).toBe(404);
  });
});
