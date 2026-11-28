// @test-type: unit — the API page's handler against an in-test reader; no live door
// @domain: principles
// @card: #5042
/**
 * Regression (DEC-2208): the answers of handlers/api-docs-viewer.ts, held to a golden.
 *   - the page for a document the door has: 200, its init script carrying the document and the
 *     read-only options (no submit methods)
 *   - a document the door does not have: 404, named
 *   - a 200 whose body is not JSON, and one that is JSON but not OpenAPI: 502, named and logged
 *   - a read that throws: 502, named and logged
 * A change to any of these answers must change the golden on purpose.
 */
import * as fs from 'fs';
import * as path from 'path';
import express from 'express';
import type { AddressInfo } from 'net';
import { API_DOCS_MOUNT, apiDocsViewer, type ReadDocument } from '../../src/handlers/api-docs-viewer';

const GOLDEN_PATH = path.join(__dirname, 'golden', 'api-docs-viewer.json');
const DOC = { openapi: '3.1.0', info: { title: 'principles API', version: '1' }, paths: { '/principles/principles': { get: { summary: 'list' } } } };

const answers: Record<string, { status: number; text: string }> = {
  '/v1/principles/principles/openapi.json': { status: 200, text: JSON.stringify(DOC) },
  '/v1/principles/garbled/openapi.json': { status: 200, text: '<html>not json</html>' },
  '/v1/principles/plain/openapi.json': { status: 200, text: '{"kind":"Discovery"}' },
};
const read: ReadDocument = async (p) => {
  if (p === '/v1/principles/down/openapi.json') throw new Error('connect ECONNREFUSED');
  return answers[p] ?? { status: 404, text: '{"error":"not-found"}' };
};

async function get(a: express.Express, p: string) {
  const server = a.listen(0, '127.0.0.1');
  await new Promise((r) => server.once('listening', r));
  try {
    const r = await fetch(`http://127.0.0.1:${(server.address() as AddressInfo).port}${p}`, { redirect: 'manual' });
    return { status: r.status, text: await r.text() };
  } finally { server.close(); }
}

test('api-docs-viewer: each answer matches the golden, and every 502 is logged with its reason', async () => {
  const logged: string[] = [];
  const a = express();
  a.use(`${API_DOCS_MOUNT}/:domain/:collection`, ...apiDocsViewer(read, (l) => logged.push(l)));
  const init = await get(a, '/athena/api-docs/principles/principles/swagger-ui-init.js');
  const options = JSON.parse(/var options = (\{[\s\S]*?\n\});/.exec(init.text)?.[1] ?? 'null');
  const refused = async (c: string) => {
    const r = await get(a, `/athena/api-docs/principles/${c}/`);
    return { status: r.status, says: /<p class="api-docs-refused">([^<]*)<\/p>/.exec(r.text)?.[1] ?? null };
  };
  const got = {
    page: { status: (await get(a, '/athena/api-docs/principles/principles/')).status },
    init: { status: init.status, swaggerDoc: options?.swaggerDoc, customOptions: options?.customOptions },
    missing: await refused('nosuch'),
    notJson: await refused('garbled'),
    notOpenApi: await refused('plain'),
    unreadable: await refused('down'),
    logged,
  };
  if (process.env.UPDATE_GOLDEN === 'true') fs.writeFileSync(GOLDEN_PATH, JSON.stringify(got, null, 2) + '\n');
  expect(got).toEqual(JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8')));
});
