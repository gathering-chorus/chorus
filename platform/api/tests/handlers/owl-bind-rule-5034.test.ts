// @test-type: integration:api — in-test upstream stubs; no live :3360 or :3393
// @domain: principles
/**
 * #5034 — the bind rule (Silas): a server.bind value becomes a fetch target only when
 * its host is in the allowed-hosts list (exact string), its port is inside the allowed
 * range, and the port is not chorus-api's own. The list and the range are the values of
 * two Property rows, found by propertyKey (owl.proxy.allowed_hosts, owl.proxy.port_range);
 * only the own-port exclusion is code. Either row missing, doubled or malformed → no
 * generated API is used at all.
 * NEGATIVE: every refused bind is shown NOT fetched — the target stub counts zero
 * requests and the path is answered by athena-make — against the same stubs that serve
 * the request when the bind is allowed.
 */
import { createServer, IncomingMessage, Server, ServerResponse } from 'http';
import type { AddressInfo } from 'net';
import app from '../../src/server';
import {
  bindRefusal,
  bindRuleFromRows,
  generatedApis,
  resetGeneratedApisCache,
  setSkippedBindLog,
} from '../../src/handlers/owl-generated-apis';

type Row = Record<string, unknown>;
type Echo = { from: string; url: string };
type Discovery = { count?: number; primitives?: Array<{ kind: string }>; unreachable?: string[] };

const HOSTS_KEY = 'owl.proxy.allowed_hosts';
const PORTS_KEY = 'owl.proxy.port_range';
/** the reason given for a bind on the port chorus-api itself listens on */
const OWN_PORT = "is this server's own";
/** the rule rows as they are in prod */
const hostsRow = (value: string): Row => ({ name: 'prop-owl-proxy-allowed-hosts', propertyKey: HOSTS_KEY, propertyValue: value });
const portsRow = (value: string): Row => ({ name: 'prop-owl-proxy-port-range', propertyKey: PORTS_KEY, propertyValue: value });
const PROD_RULE: Row[] = [hostsRow('127.0.0.1'), portsRow('1024-65535')];

let make: Server;
let target: Server;
/** every request the generated-API stub saw */
let targetHits: string[] = [];
/** the Property rows athena-make serves; each test sets them */
let propertyRows: Row[] = [];
/** what the proxy logged about binds it did not use */
let logged: string[] = [];

const json = (res: ServerResponse, body: unknown) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
};

const listen = (s: Server) => new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
const portOf = (s: Server) => (s.address() as AddressInfo).port;
const close = (s: Server) => new Promise<void>((r) => s.close(() => r()));

/** the server.bind row the `gardens` Domain row names */
const bindRow = (bind: string): Row => ({ name: 'gardens-server-bind', propertyKey: 'server.bind', propertyValue: bind });

beforeAll(async () => {
  make = createServer((req: IncomingMessage, res) => {
    const url = req.url ?? '';
    if (url === '/domains') return json(res, { data: [{ name: 'gardens', hasProperty: ['property-gardens-server-bind'] }] });
    if (url.startsWith('/properties/properties')) return json(res, { data: propertyRows });
    if (url === '/') return json(res, { kind: 'Discovery', count: 1, primitives: [{ kind: 'Domain', collection: '/v1/domains/domains' }] });
    json(res, { from: 'make', url });
  });
  target = createServer((req: IncomingMessage, res) => {
    targetHits.push(req.url ?? '');
    if (req.url === '/') return json(res, { kind: 'Discovery', primitives: [{ kind: 'Garden', collection: '/gardens/gardens' }] });
    json(res, { from: 'gardens', url: req.url });
  });
  await Promise.all([make, target].map(listen));
  process.env.OWL_UPSTREAM = `http://127.0.0.1:${portOf(make)}`;
  setSkippedBindLog((line) => logged.push(line));
});

beforeEach(() => {
  targetHits = [];
  logged = [];
});

afterAll(async () => {
  delete process.env.OWL_UPSTREAM;
  setSkippedBindLog((line) => console.warn(line));
  resetGeneratedApisCache();
  await Promise.all([make, target].map(close));
});

/** GET a path from a listening chorus-api (the caller closes it) */
async function get<T>(srv: Server, path: string): Promise<T> {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  const res = await fetch(`http://127.0.0.1:${portOf(srv)}${path}`);
  return (await res.json()) as T;
}

/** GET a path from a chorus-api started for this one request */
async function hit<T>(srv: Server, path: string): Promise<T> {
  try {
    return await get<T>(srv, path);
  } finally {
    await close(srv);
  }
}

const GARDENS = '/owl/v1/gardens/gardens';

describe('#5034 — a bind the rule allows is used', () => {
  test('127.0.0.1 on a port inside the range: the path goes to the generated API and discovery lists its kind', async () => {
    propertyRows = [...PROD_RULE, bindRow(`127.0.0.1:${portOf(target)}`)];
    resetGeneratedApisCache();
    expect(await hit<Echo>(app.listen(0), GARDENS)).toEqual({ from: 'gardens', url: '/gardens/gardens' });
    expect(targetHits).toEqual(['/gardens/gardens']);
    const d = await hit<Discovery>(app.listen(0), '/owl/');
    expect(d.primitives?.map((p) => p.kind)).toEqual(['Domain', 'Garden']);
    expect(logged).toEqual([]);
  });

  test('a comma list of hosts allows each of them, by exact string', async () => {
    propertyRows = [hostsRow('10.0.0.5, 127.0.0.1'), portsRow('1024-65535'), bindRow(`127.0.0.1:${portOf(target)}`)];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('gardens');
  });
});

describe('#5034 — NEGATIVE: a bind the rule refuses is never fetched', () => {
  test('a host not in the list, 10.0.0.5:3393 → athena-make answers, absent from discovery, logged by row and reason', async () => {
    propertyRows = [...PROD_RULE, bindRow('10.0.0.5:3393')];
    resetGeneratedApisCache();
    expect(await hit<Echo>(app.listen(0), GARDENS)).toEqual({ from: 'make', url: '/v1/gardens/gardens' });
    const d = await hit<Discovery>(app.listen(0), '/owl/');
    expect(d.primitives?.map((p) => p.kind)).toEqual(['Domain']);
    expect(d.unreachable).toBeUndefined();
    expect(targetHits).toEqual([]);
    expect(logged[0]).toContain('gardens-server-bind (host 10.0.0.5 is not in owl.proxy.allowed_hosts)');
  });

  test('localhost is not 127.0.0.1: the stub it would reach sees no request', async () => {
    propertyRows = [...PROD_RULE, bindRow(`localhost:${portOf(target)}`)];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('make');
    expect(targetHits).toEqual([]);
    expect(logged[0]).toContain('host localhost is not in');
  });

  test('a port outside the range: the same stub that answers when allowed sees no request', async () => {
    const port = portOf(target);
    // a range the OS-assigned port of the stub is not in
    const range = port < 30000 ? '30000-65535' : '1024-29999';
    propertyRows = [hostsRow('127.0.0.1'), portsRow(range), bindRow(`127.0.0.1:${port}`)];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('make');
    expect(targetHits).toEqual([]);
    expect(logged[0]).toContain(`port ${port} is outside owl.proxy.port_range ${range}`);
  });

  test('port 80 is outside 1024-65535', async () => {
    propertyRows = [...PROD_RULE, bindRow('127.0.0.1:80')];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('make');
    expect(logged[0]).toContain('port 80 is outside');
  });

  test('a bind on the port chorus-api itself listens on is a loop, and is not used', async () => {
    const srv = app.listen(0);
    await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
    propertyRows = [...PROD_RULE, bindRow(`127.0.0.1:${portOf(srv)}`)];
    resetGeneratedApisCache();
    expect(await hit<Echo>(srv, GARDENS)).toEqual({ from: 'make', url: '/v1/gardens/gardens' });
    expect(logged[0]).toContain(OWN_PORT);
    // the same bind is fine for a server on another port: the exclusion is the own port only
    expect(bindRefusal(`127.0.0.1:${portOf(target)}`, { hosts: ['127.0.0.1'], minPort: 1024, maxPort: 65535 }, portOf(make))).toBeNull();
  });

  test('a refused bind is logged once per map read, not once per request', async () => {
    propertyRows = [...PROD_RULE, bindRow('10.0.0.5:3393')];
    resetGeneratedApisCache();
    const srv = app.listen(0);
    try {
      await get<Echo>(srv, GARDENS);
      await get<Echo>(srv, GARDENS);
      await get<Discovery>(srv, '/owl/');
    } finally {
      await close(srv);
    }
    expect(logged).toHaveLength(1);
  });
});

describe('#5034 — NEGATIVE: without a readable rule no generated API is used', () => {
  const allowed = () => bindRow(`127.0.0.1:${portOf(target)}`);
  const cases: Array<[string, () => Row[], string]> = [
    ['no allowed-hosts row', () => [portsRow('1024-65535')], 'no owl.proxy.allowed_hosts row'],
    ['no port-range row', () => [hostsRow('127.0.0.1')], 'no owl.proxy.port_range row'],
    ['neither row', () => [], 'no owl.proxy.allowed_hosts row'],
    ['rows with the right names but another key', () => [
      { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'other', propertyValue: '127.0.0.1' }, portsRow('1024-65535'),
    ], 'no owl.proxy.allowed_hosts row'],
    ['an empty host list', () => [hostsRow(' , '), portsRow('1024-65535')], 'names no host'],
    ['a range that is not numbers: abc', () => [hostsRow('127.0.0.1'), portsRow('abc')], 'is not a port range'],
    ['a range past 65535 and backwards: 70000-1', () => [hostsRow('127.0.0.1'), portsRow('70000-1')], 'is not a port range'],
    ['a range that runs backwards: 6000-5000', () => [hostsRow('127.0.0.1'), portsRow('6000-5000')], 'is not a port range'],
    ['two rows carrying the hosts key', () => [hostsRow('127.0.0.1'), hostsRow('127.0.0.1'), portsRow('1024-65535')], '2 rows carry owl.proxy.allowed_hosts'],
    ['two rows carrying the range key', () => [hostsRow('127.0.0.1'), portsRow('1024-65535'), portsRow('1-65535')], '2 rows carry owl.proxy.port_range'],
  ];

  test.each(cases)('%s → the allowed-looking bind is not fetched, and the rule is logged as unreadable', async (_name, rule, reason) => {
    propertyRows = [...rule(), allowed()];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('make');
    const d = await hit<Discovery>(app.listen(0), '/owl/');
    expect(d.primitives?.map((p) => p.kind)).toEqual(['Domain']);
    expect(targetHits).toEqual([]);
    expect(logged.join('\n')).toContain('bind rule unreadable, no generated API is used');
    expect(logged.join('\n')).toContain(reason);
  });

  test('the same rows with the rule restored: the bind is fetched, so the check can tell the two states apart', async () => {
    propertyRows = [hostsRow('127.0.0.1'), allowed()];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('make');
    expect(targetHits).toEqual([]);
    propertyRows = [...PROD_RULE, allowed()];
    resetGeneratedApisCache();
    expect((await hit<Echo>(app.listen(0), GARDENS)).from).toBe('gardens');
    expect(targetHits).toEqual(['/gardens/gardens']);
  });
});

describe('#5034 — the decision itself', () => {
  const rule = { hosts: ['127.0.0.1'], minPort: 1024, maxPort: 65535 };

  test('allowed → null; each refusal names its reason', () => {
    expect(bindRefusal('127.0.0.1:3393', rule, 3340)).toBeNull();
    expect(bindRefusal('127.0.0.1:1024', rule, 3340)).toBeNull();
    expect(bindRefusal('127.0.0.1:65535', rule, 3340)).toBeNull();
    expect(bindRefusal('10.0.0.5:3393', rule, 3340)).toContain('host 10.0.0.5 is not in');
    expect(bindRefusal('localhost:3393', rule, 3340)).toContain('host localhost is not in');
    expect(bindRefusal('127.0.0.1:3340', rule, 3340)).toContain(OWN_PORT);
    expect(bindRefusal('127.0.0.1:80', rule, 3340)).toContain('port 80 is outside');
    expect(bindRefusal('127.0.0.1:1023', rule, 3340)).toContain('outside');
    expect(bindRefusal('127.0.0.1:65536', rule, 3340)).toContain('outside');
    expect(bindRefusal('127.0.0.1:3393', { missing: 'no owl.proxy.port_range row' }, 3340)).toContain('no bind rule');
  });

  test('NEGATIVE: a value that is not host:port, or that smuggles a path, a user or a second host, is refused', () => {
    for (const bind of ['', '127.0.0.1', '127.0.0.1:', '127.0.0.1:33x', '127.0.0.1:3393/x', 'evil.example@127.0.0.1:3393', '127.0.0.1:3393@evil.example', 'http://127.0.0.1:3393', '127.0.0.1:3.5', '127.0.0.1:-1']) {
      expect(bindRefusal(bind, rule, 3340)).not.toBeNull();
    }
  });

  test('the rule is read by propertyKey; the row name does not matter', () => {
    expect(bindRuleFromRows([
      { name: 'anything', propertyKey: HOSTS_KEY, propertyValue: '127.0.0.1, 10.0.0.5' },
      { name: 'whatever', propertyKey: PORTS_KEY, propertyValue: ' 5000-6000 ' },
    ])).toEqual({ hosts: ['127.0.0.1', '10.0.0.5'], minPort: 5000, maxPort: 6000 });
    expect(bindRuleFromRows([{ name: 'owl-proxy-allowed-hosts', propertyValue: '127.0.0.1' }, portsRow('5000-6000')]))
      .toEqual({ missing: 'no owl.proxy.allowed_hosts row' });
  });
});

describe('#5034 — a stale map is never served silently', () => {
  const OWL = 'http://athena-make.test';
  const T0 = 1_700_000_000_000;
  let up = true;
  /** athena-make as a function: the two listings while `up`, a refused connection otherwise */
  const read = (url: string): Promise<Response> => {
    if (!up) return Promise.reject(new Error('connect ECONNREFUSED'));
    const data = url.endsWith('/domains')
      ? [{ name: 'gardens', hasProperty: ['property-gardens-server-bind'] }]
      : [...PROD_RULE, bindRow('127.0.0.1:3393')];
    return Promise.resolve(new Response(JSON.stringify({ data }), { status: 200 }));
  };

  beforeEach(() => {
    up = true;
    resetGeneratedApisCache();
  });

  test('a read that succeeds logs nothing, and neither does a cached answer inside the minute', async () => {
    expect([...(await generatedApis(OWL, read, 3340, T0))]).toEqual([['gardens', '127.0.0.1:3393']]);
    up = false; // not read again inside the cache window, so the outage is not yet seen
    expect((await generatedApis(OWL, read, 3340, T0 + 30_000)).get('gardens')).toBe('127.0.0.1:3393');
    expect(logged).toEqual([]);
  });

  test('the read fails after the minute → the last good map is served and the line says how old it is', async () => {
    await generatedApis(OWL, read, 3340, T0);
    up = false;
    const apis = await generatedApis(OWL, read, 3340, T0 + 95_000);
    expect(apis.get('gardens')).toBe('127.0.0.1:3393');
    expect(logged).toEqual(['[owl-proxy] athena-make unreadable, serving the map read 95 s ago (connect ECONNREFUSED)']);
  });

  test('NEGATIVE: with the clock advanced the age in the line changes; a read that succeeds again resets it', async () => {
    await generatedApis(OWL, read, 3340, T0);
    up = false;
    await generatedApis(OWL, read, 3340, T0 + 95_000);
    await generatedApis(OWL, read, 3340, T0 + 200_000);
    expect(logged).toHaveLength(2);
    expect(logged[0]).toContain('read 95 s ago');
    expect(logged[1]).toContain('read 200 s ago');
    expect(logged[1]).not.toContain('95 s');
    up = true;
    await generatedApis(OWL, read, 3340, T0 + 300_000);
    expect(logged).toHaveLength(2);
    up = false;
    await generatedApis(OWL, read, 3340, T0 + 370_000);
    expect(logged[2]).toContain('read 70 s ago');
  });

  test('no map yet → no generated API, said once until a read succeeds', async () => {
    up = false;
    expect((await generatedApis(OWL, read, 3340, T0)).size).toBe(0);
    // a second read attempt, past the failure window: still the one line
    expect((await generatedApis(OWL, read, 3340, T0 + 6_000)).size).toBe(0);
    expect(logged).toEqual(['[owl-proxy] athena-make unreadable, no map yet, no generated API is used (connect ECONNREFUSED)']);
    up = true;
    expect((await generatedApis(OWL, read, 3340, T0 + 12_000)).size).toBe(1);
    expect(logged).toHaveLength(1);
  });
});

describe('#5034 — a failed map read is remembered for five seconds', () => {
  const OWL = 'http://athena-make.test';
  const T0 = 1_700_000_000_000;
  let up = true;
  /** how many times the Domain listing was asked for: one per read attempt */
  let attempts = 0;
  const read = (url: string): Promise<Response> => {
    if (url.endsWith('/domains')) attempts += 1;
    if (!up) return Promise.reject(new Error('connect ECONNREFUSED'));
    const data = url.endsWith('/domains')
      ? [{ name: 'gardens', hasProperty: ['property-gardens-server-bind'] }]
      : [...PROD_RULE, bindRow('127.0.0.1:3393')];
    return Promise.resolve(new Response(JSON.stringify({ data }), { status: 200 }));
  };

  beforeEach(() => {
    up = true;
    attempts = 0;
    resetGeneratedApisCache();
  });

  test('with no map: many requests inside the window make one read attempt; after the window, a second', async () => {
    up = false;
    for (const ms of [0, 1, 500, 2_000, 4_999]) {
      expect((await generatedApis(OWL, read, 3340, T0 + ms)).size).toBe(0);
    }
    expect(attempts).toBe(1);
    await generatedApis(OWL, read, 3340, T0 + 5_000);
    expect(attempts).toBe(2);
  });

  test('with a stale map: requests inside the window reuse it without a read, and the age line is one per attempt', async () => {
    await generatedApis(OWL, read, 3340, T0);
    up = false;
    for (const ms of [61_000, 61_500, 63_000, 65_999]) {
      expect((await generatedApis(OWL, read, 3340, T0 + ms)).get('gardens')).toBe('127.0.0.1:3393');
    }
    expect(attempts).toBe(2); // the first read, and one failed attempt
    expect(logged).toEqual(['[owl-proxy] athena-make unreadable, serving the map read 61 s ago (connect ECONNREFUSED)']);
    await generatedApis(OWL, read, 3340, T0 + 66_000);
    expect(attempts).toBe(3);
    expect(logged).toHaveLength(2);
    expect(logged[1]).toContain('read 66 s ago');
  });

  test('NEGATIVE: once athena-make answers, the next attempt after the window succeeds and replaces the map — success is not held back', async () => {
    up = false;
    expect((await generatedApis(OWL, read, 3340, T0)).size).toBe(0);
    up = true;
    // inside the window the remembered failure still answers: no read is made
    expect((await generatedApis(OWL, read, 3340, T0 + 4_000)).size).toBe(0);
    expect(attempts).toBe(1);
    // the first request past the window reads, and the map is there at once
    expect((await generatedApis(OWL, read, 3340, T0 + 5_000)).get('gardens')).toBe('127.0.0.1:3393');
    expect(attempts).toBe(2);
    // and a success clears the failure: a fresh map is served from the cache, with no further read
    expect((await generatedApis(OWL, read, 3340, T0 + 5_001)).size).toBe(1);
    expect(attempts).toBe(2);
  });
});

describe('#5034 — the map read follows every page of a listing', () => {
  const OWL = 'http://athena-make.test';
  const T0 = 1_700_000_000_000;
  /** the envelope shape athena-make really answers: data, and links.next when there is more */
  const page = (data: unknown[], next?: string) =>
    Promise.resolve(new Response(JSON.stringify({ data, links: next ? { next } : {} }), { status: 200 }));
  const filler = (n: number) => Array.from({ length: n }, (_, i) => ({ name: `other-${i}`, hasProperty: [] }));

  beforeEach(() => {
    resetGeneratedApisCache();
  });

  test('a Domain row on the second page and a rule row on the second Property page are both read', async () => {
    const asked: string[] = [];
    const read = (url: string): Promise<Response> => {
      asked.push(url);
      if (url === `${OWL}/domains`) return page(filler(100), '/v1/domains/domains?cursor=100&limit=100');
      if (url === `${OWL}/v1/domains/domains?cursor=100&limit=100`) {
        return page([{ name: 'gardens', hasProperty: ['property-gardens-server-bind'] }]);
      }
      if (url === `${OWL}/properties/properties?limit=2000`) {
        return page([hostsRow('127.0.0.1'), bindRow('127.0.0.1:3393')], '/v1/properties/properties?cursor=2&limit=2000');
      }
      if (url === `${OWL}/v1/properties/properties?cursor=2&limit=2000`) return page([portsRow('1024-65535')]);
      return Promise.resolve(new Response('{}', { status: 404 }));
    };
    expect([...(await generatedApis(OWL, read, 3340, T0))]).toEqual([['gardens', '127.0.0.1:3393']]);
    expect(asked).toContain(`${OWL}/v1/domains/domains?cursor=100&limit=100`);
    expect(logged).toEqual([]);
  });

  test('NEGATIVE: the same listings with the second pages unread would have no gardens and no rule — one page alone is refused', async () => {
    // the first pages only, with no next link: what a reader that did not page would see
    const read = (url: string): Promise<Response> =>
      url.endsWith('/domains') ? page(filler(100)) : page([hostsRow('127.0.0.1'), bindRow('127.0.0.1:3393')]);
    expect((await generatedApis(OWL, read, 3340, T0)).size).toBe(0);
    expect(logged.join('\n')).toContain('no owl.proxy.port_range row');
  });

  test('NEGATIVE: a listing that never ends is a failed read, said loudly — not a short map', async () => {
    let pages = 0;
    const read = (url: string): Promise<Response> => {
      if (url.includes('/properties/')) return page([...PROD_RULE, bindRow('127.0.0.1:3393')]);
      pages += 1;
      return page([{ name: 'gardens', hasProperty: ['property-gardens-server-bind'] }], `/v1/domains/domains?cursor=${pages}`);
    };
    expect((await generatedApis(OWL, read, 3340, T0)).size).toBe(0);
    expect(pages).toBe(20);
    expect(logged).toEqual(['[owl-proxy] athena-make unreadable, no map yet, no generated API is used (athena-make /domains did not end within 20 pages)']);
  });

  test('a page that answers non-200 partway fails the whole read', async () => {
    const read = (url: string): Promise<Response> => {
      if (url === `${OWL}/domains`) return page(filler(100), '/v1/domains/domains?cursor=100&limit=100');
      if (url.includes('cursor=100')) return Promise.resolve(new Response('{}', { status: 503 }));
      return page([...PROD_RULE, bindRow('127.0.0.1:3393')]);
    };
    expect((await generatedApis(OWL, read, 3340, T0)).size).toBe(0);
    expect(logged[0]).toContain('athena-make answered HTTP 503');
  });
});
