// @test-type: integration:api — in-test upstream stubs; no live :3360 or :3393
// @domain: principles
/**
 * #5034 — the model view (/domains, /domains/<domain>, /domains/<domain>/<instance>) is
 * one template for every path, and it answered 200 for all of them: GET
 * /domains/nosuch/thing said the address was real while the page drew "No such domain".
 * Now the status says what the model says: 200 the model has it, 404 it does not, 502
 * the model could not be read to say. The body is the template every time.
 * NEGATIVE: the same stubs with one row removed change the status, so the check can
 * tell the two states apart; an unreadable model is 502, never a guessed 200 or 404.
 */
import { createServer, IncomingMessage, Server, ServerResponse } from 'http';
import type { AddressInfo } from 'net';
import app from '../../src/server';
import { resetGeneratedApisCache } from '../../src/handlers/owl-generated-apis';

type Row = Record<string, unknown>;

/** a string only the template carries */
const TEMPLATE = 'ONE template, three levels';

let make: Server;
let principles: Server;
/** athena-make's Domain rows, and the names of its own Domain entities */
let domainRows: Row[] = [];
/** the rows principles-api serves at /principles/principles/<name> */
let principleNames: string[] = [];
/** where the Property row says principles-api listens */
let bind = '';

const json = (res: ServerResponse, status: number, body: unknown) => {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
};

function makeStub(): Server {
  return createServer((req: IncomingMessage, res) => {
    const url = req.url ?? '';
    if (url === '/domains') return json(res, 200, { data: domainRows });
    if (url.startsWith('/properties/properties')) {
      return json(res, 200, { data: [
        { name: 'principles-server-bind', propertyKey: 'server.bind', propertyValue: bind },
        // the bind rule's two rows, found by propertyKey: without them no bind is used.
        // The range starts at 1 because the "generated API down" test names port 1.
        { name: 'prop-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
        { name: 'prop-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1-65535' },
      ] });
    }
    if (url === '/') {
      return json(res, 200, { kind: 'Discovery', count: 1, primitives: [{ kind: 'Domain', collection: '/v1/domains/domains' }] });
    }
    const entity = /^\/v1\/domains\/domains\/([^/?]+)$/.exec(url)?.[1];
    if (entity && domainRows.some((r) => r.name === decodeURIComponent(entity))) return json(res, 200, { data: { name: entity } });
    json(res, 404, { error: 'not found' });
  });
}

function principlesStub(): Server {
  return createServer((req: IncomingMessage, res) => {
    const url = req.url ?? '';
    if (url === '/') return json(res, 200, { kind: 'Discovery', primitives: [{ kind: 'Principle', collection: '/principles/principles' }] });
    const entity = /^\/principles\/principles\/([^/?]+)$/.exec(url)?.[1];
    if (entity && principleNames.includes(decodeURIComponent(entity))) return json(res, 200, { data: { name: entity } });
    json(res, 404, { error: 'not found' });
  });
}

const listen = (s: Server) => new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
const bindOf = (s: Server) => `127.0.0.1:${(s.address() as AddressInfo).port}`;

/** the model as the live rows shape it: `domains` served by athena-make, `principles` by its own API */
const MODEL: Row[] = [
  { name: 'domains', definesVocabulary: 'Domain', hasProperty: [] },
  { name: 'principles', definesVocabulary: ['Principle'], hasProperty: ['property-principles-server-bind'] },
  { name: 'empty', hasProperty: [] },
];

beforeAll(async () => {
  make = makeStub();
  principles = principlesStub();
  await Promise.all([make, principles].map(listen));
});

beforeEach(() => {
  // the proxy and the view read the upstream per request, so setting it after import is honored
  process.env.OWL_UPSTREAM = `http://${bindOf(make)}`;
  bind = bindOf(principles);
  domainRows = MODEL;
  principleNames = ['hemenway-observe'];
});

afterAll(async () => {
  delete process.env.OWL_UPSTREAM;
  resetGeneratedApisCache();
  await Promise.all([make, principles].map((s) => new Promise<void>((r) => s.close(() => r()))));
});

/** GET (or HEAD) a path from a listening chorus-api, then close it */
async function hit(srv: Server, path: string, method = 'GET'): Promise<{ status: number; body: string }> {
  await new Promise<void>((r) => (srv.listening ? r() : srv.once('listening', () => r())));
  try {
    const res = await fetch(`http://127.0.0.1:${(srv.address() as AddressInfo).port}${path}`, { method });
    return { status: res.status, body: await res.text() };
  } finally {
    await new Promise<void>((r) => srv.close(() => r()));
  }
}

describe('#5034 — /domains/<domain>: the status says whether the model has the domain', () => {
  test('the catalog and a known domain answer 200 with the template', async () => {
    resetGeneratedApisCache();
    for (const path of ['/domains', '/domains/principles', '/domains/domains']) {
      const r = await hit(app.listen(0, '127.0.0.1'), path);
      expect(r.status).toBe(200);
      expect(r.body).toContain(TEMPLATE);
    }
  });

  test('an unknown domain answers 404, still with the template body', async () => {
    resetGeneratedApisCache();
    const r = await hit(app.listen(0, '127.0.0.1'), '/domains/nosuch');
    expect(r.status).toBe(404);
    expect(r.body).toContain(TEMPLATE);
    const head = await hit(app.listen(0, '127.0.0.1'), '/domains/nosuch', 'HEAD');
    expect(head.status).toBe(404);
  });

  test('NEGATIVE: the same stubs with the Domain row removed turn 200 into 404', async () => {
    resetGeneratedApisCache();
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles')).status).toBe(200);
    domainRows = MODEL.filter((r) => r.name !== 'principles');
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles')).status).toBe(404);
  });
});

describe('#5034 — /domains/<domain>/<instance>: the status says whether the domain has the row', () => {
  test('a row athena-make serves and a row a generated API serves both answer 200', async () => {
    resetGeneratedApisCache();
    let r = await hit(app.listen(0, '127.0.0.1'), '/domains/domains/principles');
    expect(r.status).toBe(200);
    expect(r.body).toContain(TEMPLATE);
    r = await hit(app.listen(0, '127.0.0.1'), '/domains/principles/hemenway-observe');
    expect(r.status).toBe(200);
    expect(r.body).toContain(TEMPLATE);
  });

  test('an unknown instance, an instance of an unknown domain, and one in a domain with no vocabulary answer 404 with the template', async () => {
    resetGeneratedApisCache();
    for (const path of ['/domains/principles/no-such-row', '/domains/domains/no-such-row', '/domains/nosuch/thing', '/domains/empty/thing']) {
      const r = await hit(app.listen(0, '127.0.0.1'), path);
      expect(r.status).toBe(404);
      expect(r.body).toContain(TEMPLATE);
    }
  });

  test('NEGATIVE: a row of another domain is not this domain’s, and removing the row turns 200 into 404', async () => {
    resetGeneratedApisCache();
    // `principles` is a Domain row, not a Principle row; `hemenway-observe` is the reverse
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles/principles')).status).toBe(404);
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/domains/hemenway-observe')).status).toBe(404);
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles/hemenway-observe')).status).toBe(200);
    principleNames = [];
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles/hemenway-observe')).status).toBe(404);
  });
});

describe('#5034 — a model that cannot be read is 502, never a guess', () => {
  test('athena-make down → 502 with the template for a domain and an instance; the catalog still 200', async () => {
    process.env.OWL_UPSTREAM = 'http://127.0.0.1:1';
    resetGeneratedApisCache();
    for (const path of ['/domains/principles', '/domains/nosuch', '/domains/principles/hemenway-observe']) {
      const r = await hit(app.listen(0, '127.0.0.1'), path);
      expect(r.status).toBe(502);
      expect(r.body).toContain(TEMPLATE);
    }
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains')).status).toBe(200);
  });

  test('the generated API of the domain down → 502 for its instances, while the domain itself is still 200', async () => {
    bind = '127.0.0.1:1';
    resetGeneratedApisCache();
    let r = await hit(app.listen(0, '127.0.0.1'), '/domains/principles/hemenway-observe');
    expect(r.status).toBe(502);
    expect(r.body).toContain(TEMPLATE);
    r = await hit(app.listen(0, '127.0.0.1'), '/domains/principles/no-such-row');
    expect(r.status).toBe(502);
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles')).status).toBe(200);
    // another domain's rows are still decided: its own upstream answers
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/domains/no-such-row')).status).toBe(404);
  });
});

describe('#5034 — a 502 from the model view is logged, with the path and the reason', () => {
  /** the lines chorus-api wrote to its error log about the model view */
  let lines: string[] = [];
  let spy: jest.SpyInstance;

  beforeEach(() => {
    lines = [];
    spy = jest.spyOn(console, 'error').mockImplementation((line: unknown) => {
      if (String(line).includes('domains-view')) lines.push(String(line));
    });
  });
  afterEach(() => {
    spy.mockRestore();
  });

  /** the JSON the line carries after its prefix */
  const fields = (line: string) => JSON.parse(line.slice(line.indexOf('{'))) as { path: string; status: number; reason: string };

  test('athena-make down → one line naming the path and that the Domain rows could not be read', async () => {
    process.env.OWL_UPSTREAM = 'http://127.0.0.1:1';
    resetGeneratedApisCache();
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles')).status).toBe(502);
    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain('[chorus-api] domains-view upstream unreadable');
    expect(fields(lines[0]).path).toBe('/domains/principles');
    expect(fields(lines[0]).status).toBe(502);
    expect(fields(lines[0]).reason).toContain('the Domain rows could not be read');
  });

  test('the generated API of the domain down → the reason names that API, a different reason from athena-make down', async () => {
    bind = '127.0.0.1:1';
    resetGeneratedApisCache();
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles/hemenway-observe')).status).toBe(502);
    expect(lines).toHaveLength(1);
    expect(fields(lines[0]).path).toBe('/domains/principles/hemenway-observe');
    expect(fields(lines[0]).reason).toBe('principles-api did not answer its discovery');
  });

  test('NEGATIVE: a 200 and a 404 log nothing — the line is for an unreadable upstream only', async () => {
    resetGeneratedApisCache();
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles')).status).toBe(200);
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/nosuch')).status).toBe(404);
    expect((await hit(app.listen(0, '127.0.0.1'), '/domains/principles/no-such-row')).status).toBe(404);
    expect(lines).toEqual([]);
  });
});
