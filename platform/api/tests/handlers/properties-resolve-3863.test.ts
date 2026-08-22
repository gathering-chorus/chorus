// @test-type: unit — pure handler, injected fetch, no live services.
/**
 * #3863 — `/api/chorus/properties/resolve` must exist and must proxy.
 *
 * Both word-cap consumers already call this route (mcp-server
 * word-cap.ts:118, chorus-hooks word_cap.rs). It has never existed:
 * `GET :3340/api/chorus/properties/resolve` returns `Cannot GET`. The consumers
 * treat any non-ok response as "fail open", so every cap Jeff mandated has been
 * a compiled constant wearing the costume of a governed value — and a missing
 * route is indistinguishable from a working one from the caller's side, which
 * is precisely why nobody noticed.
 *
 * The route PROXIES; it does not resolve. athena-make's generated `/effective`
 * walks the scope chain (#3863's other leg, landed this hour). A second
 * cascade here would compete with it and drift.
 *
 * NEGATIVE PROOF (#3734): `resolves nothing when upstream 404s` is the
 * load-bearing case. A handler that invents a default on upstream failure looks
 * identical to a working one — it returns a number — and would restore the
 * fail-open silence this card exists to end.
 */

import { resolveProperty } from '../../src/handlers/properties-resolve';

const OK = (value: unknown) =>
  ({
    ok: true,
    status: 200,
    json: async () => ({ node: 'role-wren', key: 'response.word.cap', value }),
  }) as unknown as Response;

describe('#3863 — properties/resolve proxies the generated cascade', () => {
  it('asks athena-make for the role-scoped node and returns its value', async () => {
    const calls: string[] = [];
    const fetchImpl = async (url: string) => {
      calls.push(url);
      return OK(100);
    };

    const r = await resolveProperty(
      { key: 'response.word.cap', scope: 'role', name: 'wren' },
      { fetchImpl, owlBase: 'http://localhost:3360' },
    );

    expect(r.status).toBe(200);
    expect(r.body).toMatchObject({ value: 100 });
    // scope=role + name=wren → node role-wren. Getting this wrong resolves a
    // different subject and returns a confidently wrong number.
    expect(calls[0]).toBe('http://localhost:3360/effective/role-wren/response.word.cap');
  });

  it('resolves nothing when upstream 404s — never a default', async () => {
    const fetchImpl = async () =>
      ({ ok: false, status: 404, json: async () => ({}) }) as unknown as Response;

    const r = await resolveProperty(
      { key: 'no.such.key', scope: 'role', name: 'wren' },
      { fetchImpl, owlBase: 'http://localhost:3360' },
    );

    expect(r.status).toBe(404);
    expect((r.body as { value?: unknown }).value).toBeUndefined();
  });

  it('surfaces an upstream outage as an error, not as a value', async () => {
    const fetchImpl = async () => {
      throw new Error('connect ECONNREFUSED');
    };

    const r = await resolveProperty(
      { key: 'response.word.cap', scope: 'role', name: 'wren' },
      { fetchImpl, owlBase: 'http://localhost:3360' },
    );

    expect(r.status).toBe(502);
    expect((r.body as { value?: unknown }).value).toBeUndefined();
  });

  it('refuses a request that names no key', async () => {
    const fetchImpl = async () => OK(100);
    const r = await resolveProperty(
      { key: '', scope: 'role', name: 'wren' },
      { fetchImpl, owlBase: 'http://localhost:3360' },
    );
    expect(r.status).toBe(400);
  });

  it('refuses a scope it cannot turn into a node', async () => {
    const fetchImpl = async () => OK(100);
    const r = await resolveProperty(
      { key: 'response.word.cap', scope: 'galaxy', name: 'wren' },
      { fetchImpl, owlBase: 'http://localhost:3360' },
    );
    expect(r.status).toBe(400);
  });
});
