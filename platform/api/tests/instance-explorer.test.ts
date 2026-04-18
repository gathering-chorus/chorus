/**
 * Instance Explorer static page — #2099
 *
 * Per-page migration: chorus-instance-explorer was already a static HTML
 * + D3 viz in Gathering's public/. Moved under /borg/instance-explorer/
 * with absolute paths for d3 and the viz JS (both served from /).
 *
 * Converted to in-process harness (#2173 AC4).
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2099: /borg/instance-explorer/', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('GET /borg/instance-explorer/ returns 200', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/instance-explorer/`);
    expect(res.status).toBe(200);
  });

  test('page references d3 and the explorer JS via absolute paths', async () => {
    const res = await fetch(`${harness.baseUrl}/borg/instance-explorer/`);
    const html = await res.text();
    expect(html).toContain('src="/d3.v7.min.js"');
    expect(html).toContain('src="/chorus-instance-explorer.js');
  });

  test('d3 lib is reachable at /d3.v7.min.js', async () => {
    const res = await fetch(`${harness.baseUrl}/d3.v7.min.js`);
    expect(res.status).toBe(200);
  });

  test('explorer JS is reachable at /chorus-instance-explorer.js', async () => {
    const res = await fetch(`${harness.baseUrl}/chorus-instance-explorer.js`);
    expect(res.status).toBe(200);
  });
});
