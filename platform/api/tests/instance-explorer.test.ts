/**
 * Instance Explorer static page — #2099
 *
 * Per-page migration: chorus-instance-explorer was already a static HTML
 * + D3 viz in Gathering's public/. Moved under /borg/instance-explorer/
 * with absolute paths for d3 and the viz JS (both served from /).
 */

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('#2099: /borg/instance-explorer/', () => {

  test('GET /borg/instance-explorer/ returns 200', async () => {
    const res = await fetch(`${API}/borg/instance-explorer/`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('page references d3 and the explorer JS via absolute paths', async () => {
    const res = await fetch(`${API}/borg/instance-explorer/`);
    const html = await res.text();
    expect(html).toContain('src="/d3.v7.min.js"');
    expect(html).toContain('src="/chorus-instance-explorer.js');
  }, 10_000);

  test('d3 lib is reachable at /d3.v7.min.js', async () => {
    const res = await fetch(`${API}/d3.v7.min.js`);
    expect(res.status).toBe(200);
  }, 10_000);

  test('explorer JS is reachable at /chorus-instance-explorer.js', async () => {
    const res = await fetch(`${API}/chorus-instance-explorer.js`);
    expect(res.status).toBe(200);
  }, 10_000);
});
