// @test-type: unit — the API page's handler with an in-test document reader; no live door
// @domain: principles
/**
 * #5042 AC5 — the API page (/athena/api-docs/<domain>/<collection>/) draws the generated API's
 * OpenAPI document with swagger-ui-express, read-only. Its address names only a domain and a
 * collection, so it reads only an OpenAPI document through the /owl routing, never any path a
 * caller writes. NEGATIVE: a name that is not a name reads nothing; a document the door does not
 * have is a 404 page, never an empty viewer; the viewer offers no submit method.
 */
import express, { type RequestHandler } from 'express';
import type { AddressInfo } from 'net';
import { API_DOCS_MOUNT, VIEWER_OPTIONS, apiDocsViewer, documentPath } from '../../src/handlers/api-docs-viewer';

const DOC = { openapi: '3.1.0', info: { title: 'principles API', version: '1' }, paths: { '/principles/principles': { get: { summary: 'list' } } } };
const reader = () => jest.fn(async (p: string) => (p === '/v1/principles/principles/openapi.json' ? { status: 200, body: DOC } : { status: 404, body: null }));
const mounted = (handlers: RequestHandler[]) => { const a = express(); a.use(`${API_DOCS_MOUNT}/:domain/:collection`, ...handlers); return a; };
/** One GET against the app on a port of its own; redirects are answered, not followed. */
async function get(a: express.Express, path: string) {
  const server = a.listen(0, '127.0.0.1');
  await new Promise((r) => server.once('listening', r));
  try {
    const r = await fetch(`http://127.0.0.1:${(server.address() as AddressInfo).port}${path}`, { redirect: 'manual' });
    return { status: r.status, text: await r.text(), location: r.headers.get('location') };
  } finally { server.close(); }
}

describe('#5042 the API page', () => {
  test('a domain and a collection name a document; anything else names none', () => {
    expect(documentPath('decisions', 'adrs')).toBe('/v1/decisions/adrs/openapi.json');
    // NEGATIVE: names that are not names
    for (const [d, c] of [['..', 'x'], ['principles', '..%2f..'], ['Principles', 'x'], ['a', 'b?c'], ['', 'x']]) expect(documentPath(d, c)).toBeNull();
  });

  test('the page is drawn from the document, and its init script carries it', async () => {
    const a = mounted(apiDocsViewer(reader()));
    const page = await get(a, '/athena/api-docs/principles/principles/');
    expect(page.status).toBe(200);
    expect(page.text).toMatch(/swagger-ui-init\.js/);
    const init = await get(a, '/athena/api-docs/principles/principles/swagger-ui-init.js');
    expect(init.text).toContain('"/principles/principles"');
    expect(init.text).toMatch(/"supportedSubmitMethods": \[\]/);
  });

  test('without the slash, the page sends the reader to its address with one', async () => {
    const r = await get(mounted(apiDocsViewer(reader())), '/athena/api-docs/principles/principles');
    expect(r.status).toBe(301);
    expect(r.location).toBe('principles/');
  });

  test('NEGATIVE: a document the door does not have is a 404 page; a bad name reads nothing', async () => {
    const read = reader();
    const a = mounted(apiDocsViewer(read));
    const missing = await get(a, '/athena/api-docs/principles/nosuch/');
    expect(missing.status).toBe(404);
    expect(missing.text).toMatch(/no OpenAPI document at \/v1\/principles\/nosuch\/openapi\.json/);
    read.mockClear();
    const bad = await get(a, '/athena/api-docs/Principles/x/');
    expect(bad.status).toBe(404);
    expect(read).not.toHaveBeenCalled();
  });

  test('NEGATIVE: the viewer offers no submit method', async () => {
    expect(VIEWER_OPTIONS.swaggerOptions.supportedSubmitMethods).toEqual([]);
    const init = await get(mounted(apiDocsViewer(reader())), '/athena/api-docs/principles/principles/swagger-ui-init.js');
    expect(init.text).toMatch(/"tryItOutEnabled": false/);
  });
});
