// @test-type: integration:api — in-test upstream stubs; no live :3360 or :3393
// @domain: principles
/**
 * #5017 AC2 — chorus-api's /owl proxy sends /owl/v1/principles/* to principles-api
 * (the principles domain's own generated API, served at /principles/...), and every
 * other /owl path still to athena-make. The answer equals athena-make's door row for
 * row and field for field: 14 rows, compared live before the switch (the capture in
 * fixtures/5017-live-parity.json, both doors read from the pod in the same minute).
 * NEGATIVE: a path that only starts with "principles" stays on athena-make; one changed
 * field fails the comparison, naming its row.
 * #5034 — where principles-api listens is read from athena-make's rows (the Domain's
 * server.bind Property), not from an env var: the `make` stub serves those rows.
 */
import { createServer, IncomingMessage, Server } from 'http';
import type { AddressInfo } from 'net';
import type { Express } from 'express';
import { readFileSync } from 'fs';
import { join } from 'path';
import app from '../../src/server';
import { resetGeneratedApisCache } from '../../src/handlers/owl-generated-apis';

type Row = Record<string, unknown>;
type Answer = { status: number; body: { from?: string; url?: string; error?: string; data?: Row[] } };

const live = JSON.parse(readFileSync(join(__dirname, '../fixtures/5017-live-parity.json'), 'utf8')) as {
  athenaMake: Row[];
  principlesApi: Row[];
};

let make: Server;
let principles: Server;
/** what the principles stub serves as its collection; null = echo the URL */
let served: Row[] | null = null;
/** #5034 — the bind the graph rows name for principles (host:port); the proxy reads it from them */
let bind = '';

function stub(name: 'make' | 'principles'): Server {
  return createServer((req: IncomingMessage, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    if (name === 'principles' && served) return res.end(JSON.stringify({ data: served }));
    // #5034 — athena-make's rows say where principles-api listens, in their live shapes:
    // the Domain row lists `property-<name>`, the Property row is named `<name>`
    if (name === 'make' && req.url === '/domains') {
      return res.end(JSON.stringify({ data: [{ name: 'principles', hasProperty: ['property-principles-server-bind'] }] }));
    }
    if (name === 'make' && req.url?.startsWith('/properties/properties')) {
      return res.end(JSON.stringify({ data: [
        { name: 'principles-server-bind', propertyKey: 'server.bind', propertyValue: bind },
        // #5034 — the bind rule's two rows, found by propertyKey: without them no bind is used.
        // The range starts at 1 because the "down" test below names port 1.
        { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
        { name: 'prop-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1-65535' },
      ] }));
    }
    res.end(JSON.stringify({ from: name, url: req.url }));
  });
}

const listen = (s: Server) => new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
const base = (s: Server) => `http://127.0.0.1:${(s.address() as AddressInfo).port}`;

beforeAll(async () => {
  make = stub('make');
  principles = stub('principles');
  await Promise.all([listen(make), listen(principles)]);
  // the proxy reads its upstream per request, so setting it after import is honored
  process.env.OWL_UPSTREAM = base(make);
  bind = base(principles).replace('http://', '');
  resetGeneratedApisCache();
});

afterAll(async () => {
  delete process.env.OWL_UPSTREAM;
  resetGeneratedApisCache();
  await Promise.all([make, principles].map((s) => new Promise<void>((r) => s.close(() => r()))));
});

async function hit(api: Express, path: string): Promise<Answer> {
  const srv = api.listen(0, '127.0.0.1');
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  try {
    const res = await fetch(`http://127.0.0.1:${(srv.address() as AddressInfo).port}${path}`);
    return { status: res.status, body: (await res.json()) as Answer['body'] };
  } finally {
    await new Promise<void>((r) => srv.close(() => r()));
  }
}

describe('#5017 AC2 — /owl/v1/principles/* goes to principles-api', () => {
  test('the collection and one row go to principles-api, /v1 dropped, query kept', async () => {
    let r = await hit(app, '/owl/v1/principles/principles');
    expect(r.body).toEqual({ from: 'principles', url: '/principles/principles' });
    r = await hit(app, '/owl/v1/principles/principles/hemenway-observe?x=1');
    expect(r.body).toEqual({ from: 'principles', url: '/principles/principles/hemenway-observe?x=1' });
  });

  test('every other /owl path still goes to athena-make, unchanged', async () => {
    const r = await hit(app, '/owl/v1/domains/domains');
    expect(r.body).toEqual({ from: 'make', url: '/v1/domains/domains' });
  });

  test('NEGATIVE: a path that only starts with "principles" stays on athena-make', async () => {
    const r = await hit(app, '/owl/v1/principlesx/principles');
    expect(r.body.from).toBe('make');
  });

  test('principles-api down → 502 naming principles-api, never athena-make', async () => {
    // #5034 — the rows now name a bind nothing listens on; the map is re-read from them
    const was = bind;
    bind = '127.0.0.1:1';
    resetGeneratedApisCache();
    try {
      const r = await hit(app, '/owl/v1/principles/principles');
      expect(r.status).toBe(502);
      expect(r.body.error).toMatch(/principles-api/);
      expect(r.body.error).not.toMatch(/athena-make/);
    } finally {
      bind = was;
      resetGeneratedApisCache();
    }
  });
});

/** The differences between two answers, each naming the row (and field): empty when equal. */
function differences(a: Row[], b: Row[]): string[] {
  const by = (rows: Row[]) => new Map(rows.map((r) => [String(r.name), r]));
  const [x, y] = [by(a), by(b)];
  const out: string[] = [];
  for (const name of new Set([...x.keys(), ...y.keys()])) {
    const [p, q] = [x.get(name), y.get(name)];
    if (!p || !q) {
      out.push(`${name}: only ${p ? 'athena-make' : 'principles-api'} serves it`);
      continue;
    }
    for (const k of new Set([...Object.keys(p), ...Object.keys(q)])) {
      if (JSON.stringify(p[k]) !== JSON.stringify(q[k])) out.push(`${name}.${k}`);
    }
  }
  return out;
}

describe('#5017 AC2 — through the door, principles-api equals athena-make row for row, field for field', () => {
  afterEach(() => {
    served = null;
  });

  test('14 rows through /owl/v1/principles/principles, no difference from athena-make', async () => {
    served = live.principlesApi;
    const r = await hit(app, '/owl/v1/principles/principles');
    expect(r.body.data).toHaveLength(14);
    expect(live.athenaMake).toHaveLength(14);
    expect(differences(live.athenaMake, r.body.data ?? [])).toEqual([]);
  });

  test('NEGATIVE: one changed field fails, naming its row and field; a missing row is named', async () => {
    const first = String(live.principlesApi[0].name);
    served = live.principlesApi.map((r, i) => (i === 0 ? { ...r, label: 'changed' } : r));
    let r = await hit(app, '/owl/v1/principles/principles');
    expect(differences(live.athenaMake, r.body.data ?? [])).toEqual([`${first}.label`]);
    served = live.principlesApi.slice(1);
    r = await hit(app, '/owl/v1/principles/principles');
    expect(differences(live.athenaMake, r.body.data ?? [])).toEqual([`${first}: only athena-make serves it`]);
  });
});
