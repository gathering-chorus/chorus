// @test-type: unit — injected fake fetch; no athena-make, no Fuseki, brings its own world.
/**
 * fetchLoomPrinciples — #3749 rewrite. The handler now sources the generated
 * athena-make surface (list + entity reads) instead of the retired SPARQL path
 * that read the wrong graph for months (the 2-of-29 split). Legacy envelope
 * shape preserved for loom/principles.html + the session-boot injector.
 */
import { fetchLoomPrinciples } from '../../src/handlers/loom-principles';

const list = (names: string[]) => ({
  ok: true, status: 200,
  json: async () => ({ data: names.map((n) => ({ name: n, label: '', comment: '', status: '' })) }),
}) as unknown as Response;

const entity = (name: string, fields: Record<string, string>) => ({
  ok: true, status: 200,
  json: async () => ({ data: { iri: `https://jeffbridwell.com/chorus#${name}`, ...fields } }),
}) as unknown as Response;

describe('#3749 fetchLoomPrinciples via athena-make', () => {
  test('folds list + entity reads into the legacy envelope shape', async () => {
    const fetchFn = jest.fn(async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.endsWith('/principles')) return list(['hemenway-observe', 'hemenway-connect']);
      if (u.includes('hemenway-observe')) return entity('hemenway-observe', { label: 'Observe', comment: 'Watch first.', techReading: 'Gemba.', jeffReading: 'Reads the board.', isPermacultureParent: 'true' });
      return entity('hemenway-connect', { label: 'Connect', comment: 'Relative location.', isPermacultureParent: 'true' });
    });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(200);
    const body = r.body as { data: { principles: Array<Record<string, unknown>> } };
    expect(body.data.principles).toHaveLength(2);
    const obs = body.data.principles.find((p) => p.id === 'hemenway-observe')!;
    expect(obs.label).toBe('Observe');
    expect(obs.techReading).toBe('Gemba.');
    expect(obs.isPermacultureParent).toBe(true);
    expect(obs.parents).toEqual([]); // the 14 are peers post-#3749
  });

  test('NEGATIVE: athena-make unreachable → 502 error envelope, NEVER an empty principle list', async () => {
    const fetchFn = jest.fn(async () => { throw new Error('ECONNREFUSED'); });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(502);
    const body = r.body as { data: { error?: string; principles?: unknown[] } };
    expect(body.data.error).toBeTruthy();
    expect(body.data.principles).toBeUndefined();
  });

  test('NEGATIVE: athena-make non-200 on the list → 502, the status is named', async () => {
    const fetchFn = jest.fn(async () => ({ ok: false, status: 503, json: async () => ({}) }) as unknown as Response);
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    expect(r.status).toBe(502);
    expect(JSON.stringify(r.body)).toContain('503');
  });

  test('a single vanished entity mid-walk is skipped; the count tells the truth', async () => {
    const fetchFn = jest.fn(async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.endsWith('/principles')) return list(['hemenway-observe', 'hemenway-gone']);
      if (u.includes('hemenway-observe')) return entity('hemenway-observe', { label: 'Observe', comment: 'c' });
      return { ok: false, status: 404, json: async () => ({}) } as unknown as Response;
    });
    const r = await fetchLoomPrinciples({ fetchFn: fetchFn as unknown as typeof fetch, owlApiUrl: 'http://owl' });
    const body = r.body as { data: { principles: unknown[] }; _meta: { count: number } };
    expect(body.data.principles).toHaveLength(1);
    expect(body._meta.count).toBe(1);
  });
});
