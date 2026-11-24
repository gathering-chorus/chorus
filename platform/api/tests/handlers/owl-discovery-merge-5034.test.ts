// @test-type: integration:api — in-test upstream stubs; no live :3360 or :3393
// @domain: principles
/**
 * #5034 — the discovery document at /owl/ lists what every generated API serves, and
 * which domains have a generated API is read from the graph rows, not named in code.
 *
 * athena-make's discovery never listed a class served by a domain's own API, so the
 * Athena pages said "Principle: Not served". Here: a Domain row whose hasProperty names
 * a Property row with propertyKey "server.bind" is a generated API; its own discovery
 * (GET / at the bind) is merged in, its collection given the public /v1 path.
 * NEGATIVE: with no Domain row carrying a server.bind Property, Principle is absent and
 * /owl/v1/principles/... stays on athena-make — the same stubs, one row different.
 * A generated API that is down is named in `unreachable`, never dropped in silence.
 */
import { createServer, IncomingMessage, Server, ServerResponse } from 'http';
import type { AddressInfo } from 'net';
import app from '../../src/server';
import { resetGeneratedApisCache, setReadTimeoutMs } from '../../src/handlers/owl-generated-apis';

type Primitive = { kind: string; collection: string; composed?: boolean };
type Discovery = { kind?: string; count?: number; primitives?: Primitive[]; unreachable?: string[] };
type Echo = { from: string; url: string };
type Row = Record<string, unknown>;

/** athena-make's own discovery entries */
const MAKE_PRIMITIVES: Primitive[] = [
  { kind: 'Domain', collection: '/v1/domains/domains', composed: false },
  { kind: 'ADR', collection: '/v1/decisions/adrs', composed: false },
];

let make: Server;
let principles: Server;
let gardens: Server;
/** the rows athena-make serves; each test sets them */
let domainRows: Row[] = [];
let propertyRows: Row[] = [];
/** what the principles stub lists at GET / */
let principlesPrimitives: Primitive[] = [];
/** a server that accepts the request and never answers */
let hung: Server;
/** when true, athena-make accepts GET / (its discovery) and never answers; its rows still read */
let hangDiscovery = false;

const json = (res: ServerResponse, body: unknown) => {
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
};

function makeStub(): Server {
  return createServer((req: IncomingMessage, res) => {
    const url = req.url ?? '';
    if (url === '/domains') return json(res, { data: domainRows });
    if (url.startsWith('/properties/properties')) return json(res, { data: propertyRows });
    if (url === '/' || url.startsWith('/?')) {
      if (hangDiscovery) return;
      return json(res, { kind: 'Discovery', count: MAKE_PRIMITIVES.length, primitives: MAKE_PRIMITIVES });
    }
    json(res, { from: 'make', url });
  });
}

/** a generated API: its own discovery at /, everything else echoed */
function apiStub(name: string, primitives: () => Primitive[]): Server {
  return createServer((req: IncomingMessage, res) => {
    if (req.url === '/') return json(res, { kind: 'Discovery', primitives: primitives() });
    json(res, { from: name, url: req.url });
  });
}

const listen = (s: Server) => new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
const bindOf = (s: Server) => `127.0.0.1:${(s.address() as AddressInfo).port}`;

/** the two rows that make `domain` a generated API at `bind`, in the live shapes */
const generated = (domain: string, bind: string): { domain: Row; property: Row } => ({
  // the Domain row lists `property-<name>`; the Property row is named `<name>`
  domain: { name: domain, hasProperty: [`property-${domain}-server-bind`, `property-${domain}-other`] },
  property: { name: `${domain}-server-bind`, propertyKey: 'server.bind', propertyValue: bind },
});

/**
 * The bind rule's two rows, found by propertyKey: without them no bind is used.
 * The range starts at 1 because the "down" test names port 1 (owl-bind-rule-5034 tests the rule itself).
 */
const RULE: Row[] = [
  { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
  { name: 'prop-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1-65535' },
];

/** set the rows athena-make serves: one plain domain, plus the given generated APIs */
function rows(...apis: Array<{ domain: Row; property: Row }>): void {
  domainRows = [{ name: 'domains', hasProperty: 'property-domains-owner' }, ...apis.map((a) => a.domain)];
  propertyRows = [
    { name: 'domains-owner', propertyKey: 'owner', propertyValue: 'kade' },
    ...RULE,
    ...apis.map((a) => a.property),
  ];
}

beforeAll(async () => {
  make = makeStub();
  principles = apiStub('principles', () => principlesPrimitives);
  gardens = apiStub('gardens', () => [{ kind: 'Garden', collection: '/gardens/gardens', composed: false }]);
  hung = createServer(() => { /* never answers */ });
  await Promise.all([make, principles, gardens, hung].map(listen));
  // the proxy reads its upstream per request, so setting it after import is honored
  process.env.OWL_UPSTREAM = `http://${bindOf(make)}`;
});

beforeEach(() => {
  principlesPrimitives = [{ kind: 'Principle', collection: '/principles/principles', composed: false }];
  hangDiscovery = false;
});

afterAll(async () => {
  delete process.env.OWL_UPSTREAM;
  setReadTimeoutMs();
  resetGeneratedApisCache();
  // the requests left unanswered still hold their sockets
  for (const s of [make, hung]) s.closeAllConnections();
  await Promise.all([make, principles, gardens, hung].map((s) => new Promise<void>((r) => s.close(() => r()))));
});

/** GET a path from a listening chorus-api, then close it */
async function hit<T>(srv: Server, path: string): Promise<{ status: number; body: T }> {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  try {
    const res = await fetch(`http://127.0.0.1:${(srv.address() as AddressInfo).port}${path}`);
    return { status: res.status, body: (await res.json()) as T };
  } finally {
    await new Promise<void>((r) => srv.close(() => r()));
  }
}

const kinds = (d: Discovery) => (d.primitives ?? []).map((p) => p.kind);

describe('#5034 — /owl/ discovery lists what the generated APIs serve', () => {
  test('Principle is listed at /v1/principles/principles beside the entries of athena-make, count correct', async () => {
    rows(generated('principles', bindOf(principles)));
    resetGeneratedApisCache();
    for (const path of ['/owl/', '/owl', '/owl/?x=1']) {
      const r = await hit<Discovery>(app.listen(0), path);
      expect(r.status).toBe(200);
      expect(r.body.kind).toBe('Discovery');
      expect(r.body.primitives).toEqual([
        ...MAKE_PRIMITIVES,
        { kind: 'Principle', collection: '/v1/principles/principles', composed: false },
      ]);
      expect(r.body.count).toBe(3);
      expect(r.body.unreachable).toBeUndefined();
    }
  });

  test('NEGATIVE: no Domain row carries a server.bind Property → Principle is absent, and its path stays on athena-make', async () => {
    // the principles stub is up and would answer; only the rows differ from the test above
    rows();
    resetGeneratedApisCache();
    const r = await hit<Discovery>(app.listen(0), '/owl/');
    expect(r.status).toBe(200);
    expect(kinds(r.body)).toEqual(['Domain', 'ADR']);
    expect(r.body.count).toBe(2);
    const row = await hit<Echo>(app.listen(0), '/owl/v1/principles/principles');
    expect(row.body.from).toBe('make');
  });

  test('NEGATIVE: a Property named by the Domain but with another key is not a bind', async () => {
    const g = generated('principles', bindOf(principles));
    rows({ domain: g.domain, property: { ...g.property, propertyKey: 'server.port' } });
    resetGeneratedApisCache();
    const r = await hit<Discovery>(app.listen(0), '/owl/');
    expect(kinds(r.body)).toEqual(['Domain', 'ADR']);
  });

  test('a generated API that is down → still 200 with the entries of athena-make, and it is named in unreachable', async () => {
    rows(generated('principles', '127.0.0.1:1'), generated('gardens', bindOf(gardens)));
    resetGeneratedApisCache();
    const r = await hit<Discovery>(app.listen(0), '/owl/');
    expect(r.status).toBe(200);
    expect(kinds(r.body)).toEqual(['Domain', 'ADR', 'Garden']);
    expect(r.body.count).toBe(3);
    expect(r.body.unreachable).toEqual(['principles']);
  });

  test('a kind athena-make already lists is not listed twice', async () => {
    principlesPrimitives = [
      { kind: 'ADR', collection: '/principles/adrs', composed: false },
      { kind: 'Principle', collection: '/principles/principles', composed: false },
    ];
    rows(generated('principles', bindOf(principles)));
    resetGeneratedApisCache();
    const r = await hit<Discovery>(app.listen(0), '/owl/');
    expect(kinds(r.body)).toEqual(['Domain', 'ADR', 'Principle']);
    expect(r.body.primitives?.find((p) => p.kind === 'ADR')?.collection).toBe('/v1/decisions/adrs');
    expect(r.body.count).toBe(3);
  });
});

describe('#5034 — which domains have a generated API comes from the rows', () => {
  test('a second domain with its own bind is routed to its own API, /v1 dropped, query kept', async () => {
    rows(generated('principles', bindOf(principles)), generated('gardens', bindOf(gardens)));
    resetGeneratedApisCache();
    let r = await hit<Echo>(app.listen(0), '/owl/v1/gardens/gardens/bed-1?x=1');
    expect(r.body).toEqual({ from: 'gardens', url: '/gardens/gardens/bed-1?x=1' });
    r = await hit<Echo>(app.listen(0), '/owl/v1/principles/principles');
    expect(r.body).toEqual({ from: 'principles', url: '/principles/principles' });
    const d = await hit<Discovery>(app.listen(0), '/owl/');
    expect(kinds(d.body)).toEqual(['Domain', 'ADR', 'Principle', 'Garden']);
    expect(d.body.count).toBe(4);
  });

  test('NEGATIVE: a path that only starts with the name of the domain, and the domain once its row is gone, stay on athena-make', async () => {
    rows(generated('gardens', bindOf(gardens)));
    resetGeneratedApisCache();
    let r = await hit<Echo>(app.listen(0), '/owl/v1/gardensx/gardens');
    expect(r.body).toEqual({ from: 'make', url: '/v1/gardensx/gardens' });
    rows();
    resetGeneratedApisCache();
    r = await hit<Echo>(app.listen(0), '/owl/v1/gardens/gardens');
    expect(r.body).toEqual({ from: 'make', url: '/v1/gardens/gardens' });
  });

  test('hasProperty as one string, and the Property named with the property- prefix, both match', async () => {
    domainRows = [{ name: 'gardens', hasProperty: 'gardens-server-bind' }];
    propertyRows = [...RULE, { name: 'property-gardens-server-bind', propertyKey: 'server.bind', propertyValue: bindOf(gardens) }];
    resetGeneratedApisCache();
    const r = await hit<Echo>(app.listen(0), '/owl/v1/gardens/gardens');
    expect(r.body.from).toBe('gardens');
  });
});

describe('#5034 — an upstream that accepts the request and never answers does not hang the page', () => {
  /** the injected read timeout, and the most a request may take: a small multiple of it */
  const TIMEOUT_MS = 150;
  const WITHIN_MS = TIMEOUT_MS * 8;

  /** GET a path; the status, the body as text, and how long it took */
  async function timed(srv: Server, path: string): Promise<{ status: number; text: string; ms: number }> {
    await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
    const started = Date.now();
    try {
      const res = await fetch(`http://127.0.0.1:${(srv.address() as AddressInfo).port}${path}`);
      const text = await res.text();
      return { status: res.status, text, ms: Date.now() - started };
    } finally {
      srv.closeAllConnections();
      await new Promise<void>((r) => srv.close(() => r()));
    }
  }

  afterEach(() => {
    setReadTimeoutMs();
  });

  test('athena-make never answers its discovery → 502 naming athena-make, inside the timeout', async () => {
    rows(generated('principles', bindOf(principles)));
    resetGeneratedApisCache();
    hangDiscovery = true;
    setReadTimeoutMs(TIMEOUT_MS);
    const r = await timed(app.listen(0), '/owl/');
    expect(r.status).toBe(502);
    expect(r.text).toContain('athena-make unreachable');
    expect(r.ms).toBeLessThan(WITHIN_MS);
  });

  test('a generated API never answers its discovery → still 200, and it is named in unreachable, inside the timeout', async () => {
    rows(generated('principles', bindOf(hung)), generated('gardens', bindOf(gardens)));
    resetGeneratedApisCache();
    setReadTimeoutMs(TIMEOUT_MS);
    const r = await timed(app.listen(0), '/owl/');
    expect(r.status).toBe(200);
    const d = JSON.parse(r.text) as Discovery;
    expect(kinds(d)).toEqual(['Domain', 'ADR', 'Garden']);
    expect(d.unreachable).toEqual(['principles']);
    expect(r.ms).toBeLessThan(WITHIN_MS);
  });

  test('the model view of an instance → 502 when the discovery never answers, inside the timeout', async () => {
    rows(generated('principles', bindOf(principles)));
    resetGeneratedApisCache();
    hangDiscovery = true;
    setReadTimeoutMs(TIMEOUT_MS);
    const r = await timed(app.listen(0), '/domains/principles/hemenway-observe');
    expect(r.status).toBe(502);
    expect(r.ms).toBeLessThan(WITHIN_MS);
  });

  test('NEGATIVE: the same stubs answering → 200 with Principle listed, so the 502 above is the timeout and nothing else', async () => {
    rows(generated('principles', bindOf(principles)));
    resetGeneratedApisCache();
    setReadTimeoutMs(TIMEOUT_MS);
    const r = await timed(app.listen(0), '/owl/');
    expect(r.status).toBe(200);
    expect(kinds(JSON.parse(r.text) as Discovery)).toEqual(['Domain', 'ADR', 'Principle']);
  });
});
