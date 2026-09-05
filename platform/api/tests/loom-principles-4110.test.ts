// @test-type: unit
/**
 * #4110 — a principles read that loses every row must not read as "no principles".
 *
 * The live failure this covers: athena-make's collection served 28 principles
 * while every entity read under it answered 404. The handler skipped each one
 * and returned 200 with an empty array, so every session booted with no
 * principles and nothing anywhere went red. The only trace was a
 * session.principles.empty event carrying error_type=api_returned_empty_set,
 * which reads like a fact about the data rather than a broken read.
 *
 * The three states below are the point. A check that cannot separate them is
 * the defect, not the fix:
 *
 *   named 0, resolved 0   genuinely empty        -> 200, empty
 *   named N, resolved 0   broken entity reads    -> 502, names the counts
 *   named N, resolved N   healthy                -> 200, N rows
 */
import { fetchLoomPrinciples } from '../src/handlers/loom-principles';

type Row = Record<string, string>;

/** A fake athena-make: `names` is what the collection reports, `entities` what resolves. */
function fakeUpstream(names: string[], entities: Record<string, Row>) {
  return async (url: string | URL | Request): Promise<Response> => {
    const href = typeof url === 'string' ? url : url.toString();
    if (href.endsWith('/principles')) {
      return new Response(JSON.stringify({ data: names.map((n) => ({ name: n })) }), { status: 200 });
    }
    const name = decodeURIComponent(href.split('/principles/')[1] ?? '');
    const row = entities[name];
    if (!row) return new Response(JSON.stringify({ kind: 'Error' }), { status: 404 });
    return new Response(JSON.stringify({ data: row }), { status: 200 });
  };
}

const body = (r: { body: unknown }) => r.body as { _meta: Record<string, unknown>; data: Record<string, unknown> };

describe('#4110 loom principles — an unresolvable walk is a broken read, not an empty set', () => {
  it('NEGATIVE PROOF: collection names rows but every entity 404s -> 502 naming the counts', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(['a', 'b', 'c'], {}) as unknown as typeof fetch,
    });

    expect(res.status).toBe(502);
    expect(body(res)._meta.named).toBe(3);
    expect(body(res)._meta.resolved).toBe(0);
    expect(String(body(res).data.error)).toContain('none could be read');
  });

  it('a genuinely empty collection is still a valid 200, not an error', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream([], {}) as unknown as typeof fetch,
    });

    expect(res.status).toBe(200);
    expect((body(res).data.principles as unknown[]).length).toBe(0);
  });

  it('a healthy walk still serves its rows', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(['get-a-yield'], {
        'get-a-yield': { label: 'Obtain a yield', comment: 'c', order: '3' },
      }) as unknown as typeof fetch,
    });

    expect(res.status).toBe(200);
    const rows = body(res).data.principles as Array<{ id: string; label: string }>;
    expect(rows).toHaveLength(1);
    expect(rows[0].label).toBe('Obtain a yield');
  });

  it('one row vanishing mid-walk is still tolerated — the skip was right for that case', async () => {
    const res = await fetchLoomPrinciples({
      fetchFn: fakeUpstream(['here', 'gone'], {
        here: { label: 'Still here', comment: 'c', order: '1' },
      }) as unknown as typeof fetch,
    });

    expect(res.status).toBe(200);
    expect((body(res).data.principles as unknown[]).length).toBe(1);
  });
});
