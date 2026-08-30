// @test-type: unit — stubs global fetch; no Fuseki, no live service, brings its own world.
// #4030 — the /api/chorus/class-atlas route (#3992). Jeff's experience under
// test: the Class Atlas tile either shows the model's classes from the store,
// or says plainly that the store could not be asked — an empty atlas must
// never read as "the model has no classes".
import { classAtlasHandler } from '../src/handlers/class-atlas';

type Sent = { status: number; body: unknown };
function fakeRes(): { res: any; sent: Sent } {
  const sent: Sent = { status: 200, body: undefined };
  const res = {
    status(n: number) { sent.status = n; return res; },
    json(b: unknown) { sent.body = b; return res; },
  };
  return { res, sent };
}

const NS = 'https://jeffbridwell.com/chorus#';
const bindings = [
  { domain: { value: `${NS}domain-board` }, class: { value: `${NS}Card` },
    prop: { value: `${NS}ownedBy` }, rc: { value: `${NS}Role` } },
  { domain: { value: `${NS}domain-roles` }, class: { value: `${NS}Role` } },
];

const realFetch = global.fetch;
afterEach(() => { global.fetch = realFetch; delete process.env.CHORUS_FUSEKI; });

describe('GET /api/chorus/class-atlas', () => {
  it('answers the atlas from the store, marking storeReachable and cross-domain edges', async () => {
    let asked = '';
    global.fetch = (async (url: string, init: { body: string }) => {
      asked = url + ' ' + decodeURIComponent(init.body);
      return { ok: true, status: 200, json: async () => ({ results: { bindings } }) };
    }) as unknown as typeof fetch;
    process.env.CHORUS_FUSEKI = 'http://store.test/pods';
    const { res, sent } = fakeRes();
    await classAtlasHandler()({} as any, res);
    expect(sent.status).toBe(200);
    const body = sent.body as { storeReachable: boolean; graph: string; domains: Array<{ name: string; classes: Array<{ name: string; edges: Array<{ to: string; crossDomain: boolean }> }> }> };
    expect(body.storeReachable).toBe(true);
    expect(body.graph).toBe('urn:chorus:ontology');
    expect(asked).toContain('http://store.test/pods/query');
    expect(asked).toContain('urn:chorus:ontology');
    const board = body.domains.find((d) => d.name === 'domain-board')!;
    expect(board.classes[0].edges).toEqual([{ name: 'ownedBy', to: 'Role', multiplicity: '0..*', crossDomain: true }]);
  });

  it('store answers non-2xx → 502 store-unreachable carrying the HTTP status, never an empty atlas', async () => {
    global.fetch = (async () => ({ ok: false, status: 503, json: async () => ({}) })) as unknown as typeof fetch;
    const { res, sent } = fakeRes();
    await classAtlasHandler()({} as any, res);
    expect(sent.status).toBe(502);
    expect(sent.body).toEqual({ error: 'store-unreachable', storeReachable: false, http: 503 });
  });

  it('store connection fails → 502 store-unreachable, no throw', async () => {
    global.fetch = (async () => { throw new Error('ECONNREFUSED'); }) as unknown as typeof fetch;
    const { res, sent } = fakeRes();
    await classAtlasHandler()({} as any, res);
    expect(sent.status).toBe(502);
    expect(sent.body).toEqual({ error: 'store-unreachable', storeReachable: false });
  });
});
