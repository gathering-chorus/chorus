// @test-type: unit — injected fake fetch; no athena-make, no generated API, brings its own world.
// @domain: principles
/**
 * #5034 — the class atlas's "served" column read athena-make's discovery by itself, a
 * second reader the merge did not cover: a class served by a domain's own generated
 * API (Principle) read as not served. It also stripped /v1 from each collection before
 * counting it. fetchServed now reads the ONE merged discovery and counts each
 * collection at the path discovery names, at the upstream the /owl proxy would use.
 * NEGATIVE: the same world without the server.bind row, or with the generated API
 * down, does NOT list Principle — "not confirmed", never "served" on a guess.
 */
import { annotateServed, fetchServed, type Atlas } from '../src/handlers/class-atlas';
import { resetGeneratedApisCache, setSkippedBindLog, type FetchLike } from '../src/handlers/owl-generated-apis';

const MAKE = 'http://athena-make.test';
const PRINCIPLES_API = 'http://127.0.0.1:4777';

const RULE = [
  { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
  { name: 'prop-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1024-65535' },
];
const BIND = { name: 'principles-server-bind', propertyKey: 'server.bind', propertyValue: '127.0.0.1:4777' };

const reply = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

type World = {
  /** the Property rows athena-make serves */
  properties: unknown[];
  /** false: the generated API refuses the connection */
  principlesUp: boolean;
  /** what the generated API answers when asked to count its collection */
  principlesCount: () => Response;
  /** false: athena-make refuses its discovery */
  discoveryUp: boolean;
};

function world(over: Partial<World> = {}): { asked: string[]; fetchImpl: FetchLike } {
  const w: World = {
    properties: [...RULE, BIND],
    principlesUp: true,
    principlesCount: () => reply({ count: 14, data: [] }),
    discoveryUp: true,
    ...over,
  };
  const asked: string[] = [];
  const fetchImpl: FetchLike = (url) => {
    asked.push(url);
    if (url.startsWith(PRINCIPLES_API)) {
      if (!w.principlesUp) return Promise.reject(new Error('ECONNREFUSED'));
      if (url === `${PRINCIPLES_API}/`) {
        return Promise.resolve(reply({ kind: 'Discovery', primitives: [{ kind: 'Principle', collection: '/principles/principles' }] }));
      }
      return Promise.resolve(w.principlesCount());
    }
    if (url === `${MAKE}/domains`) {
      return Promise.resolve(reply({ data: [{ name: 'principles', hasProperty: ['property-principles-server-bind'] }] }));
    }
    if (url.startsWith(`${MAKE}/properties/properties`)) return Promise.resolve(reply({ data: w.properties }));
    if (url === `${MAKE}/`) {
      if (!w.discoveryUp) return Promise.reject(new Error('ECONNREFUSED'));
      return Promise.resolve(reply({
        kind: 'Discovery',
        count: 2,
        primitives: [
          { kind: 'Domain', collection: '/v1/domains/domains' },
          { kind: 'SpineEvent', collection: '/v1/events/spineevents' },
        ],
      }));
    }
    if (url === `${MAKE}/v1/domains/domains?limit=1&count=true`) return Promise.resolve(reply({ count: 64, data: [] }));
    if (url === `${MAKE}/v1/events/spineevents?limit=1&count=true`) return Promise.resolve(reply({ count: 0, data: [] }));
    return Promise.resolve(reply({ error: 'not found' }, 404));
  };
  return { asked, fetchImpl };
}

beforeEach(() => {
  resetGeneratedApisCache();
  setSkippedBindLog(() => { /* refusals are asserted by what is listed; owl-bind-rule-5034 reads the lines */ });
});
afterAll(() => {
  setSkippedBindLog((line) => console.warn(line));
  resetGeneratedApisCache();
});

describe('#5034 the atlas reads the merged discovery', () => {
  test('a kind served by a generated API is listed beside the kinds of athena-make, each with its count', async () => {
    const w = world();
    const served = await fetchServed(MAKE, 3340, w.fetchImpl);
    expect(served).toEqual([
      { kind: 'Domain', collection: '/v1/domains/domains', count: 64 },
      { kind: 'SpineEvent', collection: '/v1/events/spineevents', count: 0 },
      { kind: 'Principle', collection: '/v1/principles/principles', count: 14 },
    ]);
  });

  test('each collection is counted at the path discovery names: /v1 kept on athena-make, dropped only on the way to the generated API', async () => {
    const w = world();
    await fetchServed(MAKE, 3340, w.fetchImpl);
    expect(w.asked).toContain(`${MAKE}/v1/domains/domains?limit=1&count=true`);
    expect(w.asked).toContain(`${PRINCIPLES_API}/principles/principles?limit=1&count=true`);
    // the old reader stripped /v1 and asked athena-make for everything
    expect(w.asked).not.toContain(`${MAKE}/domains/domains?limit=1&count=true`);
    expect(w.asked).not.toContain(`${MAKE}/principles/principles?limit=1&count=true`);
  });

  test('the atlas marks Principle served, with its row count', async () => {
    const atlas: Atlas = { domains: [{ name: 'principles', classes: [{ name: 'Principle', attributes: [], edges: [], parents: [] }] }] };
    const out = annotateServed(atlas, await fetchServed(MAKE, 3340, world().fetchImpl));
    expect(out.domains[0].classes[0].served).toBe(true);
    expect(out.domains[0].classes[0].rowCount).toBe(14);
  });

  test('a generated API that will not count its collection is still served; the count is absent, not 0', async () => {
    // measured on principles-api 2026-11-24: ?count=true answers {"error":"unknown-field"}
    const w = world({ principlesCount: () => reply({ error: 'unknown-field', message: 'Principle has no field count' }, 400) });
    const served = await fetchServed(MAKE, 3340, w.fetchImpl);
    expect(served.find((s) => s.kind === 'Principle')).toEqual({ kind: 'Principle', collection: '/v1/principles/principles' });
  });
});

describe('#5034 NEGATIVE: Principle is listed only when its API is found and answers', () => {
  test('no server.bind row → Principle is not listed, and nothing is asked of a generated API', async () => {
    const w = world({ properties: RULE });
    const served = await fetchServed(MAKE, 3340, w.fetchImpl);
    expect(served.map((s) => s.kind)).toEqual(['Domain', 'SpineEvent']);
    expect(w.asked.some((u) => u.startsWith(PRINCIPLES_API))).toBe(false);
  });

  test('the generated API down → Principle is not confirmed, the kinds of athena-make still are', async () => {
    const w = world({ principlesUp: false });
    const served = await fetchServed(MAKE, 3340, w.fetchImpl);
    expect(served.map((s) => s.kind)).toEqual(['Domain', 'SpineEvent']);
    const atlas: Atlas = { domains: [{ name: 'principles', classes: [{ name: 'Principle', attributes: [], edges: [], parents: [] }] }] };
    expect(annotateServed(atlas, served).domains[0].classes[0].served).toBe(false);
  });

  test('athena-make will not answer its discovery → nothing is confirmed served', async () => {
    const w = world({ discoveryUp: false });
    expect(await fetchServed(MAKE, 3340, w.fetchImpl)).toEqual([]);
  });
});
