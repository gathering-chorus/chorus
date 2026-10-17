// @test-type: unit
// @domain: messages
// @card: #4571
// @owner: wren
// #4571 — Jeff's ruling via Silas 20:02: v2 is reached at lightlifeurbangardens.com/clearing-v2,
// and the tunnel does not strip the prefix (same as /clearing, #3872). Every route must answer
// under both '/' (LAN :3472) and '/clearing-v2', and the page must build its URLs with the prefix.
import http from 'http';
import { createApp } from '../src/server';
import { stripBase, withBase } from '../src/base-path';

async function withApp(fn: (port: number) => Promise<void>) {
  const { app } = createApp({ relayUrl: '', pulseUrl: 'http://127.0.0.1:9', relaySigner: null });
  const server = http.createServer(app);
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  try { await fn((server.address() as { port: number }).port); } finally { await new Promise<void>((r) => server.close(() => r())); }
}

test('health and the page answer under /clearing-v2 and under /', async () => {
  await withApp(async (port) => {
    expect((await fetch(`http://127.0.0.1:${port}/clearing-v2/health`)).status).toBe(200);
    expect((await fetch(`http://127.0.0.1:${port}/health`)).status).toBe(200);
    const page = await (await fetch(`http://127.0.0.1:${port}/clearing-v2/`)).text();
    expect(page).toContain('window.CLEARING_BASE="/clearing-v2"');
    expect(page).not.toMatch(/(src|href)="\/(?!\/|clearing-v2)/);
    const lan = await (await fetch(`http://127.0.0.1:${port}/`)).text();
    expect(lan).toContain('window.CLEARING_BASE=""');
  });
});

test('negative proof: a look-alike prefix is not stripped, and protocol-relative URLs are left alone', () => {
  expect(stripBase('/clearing-v2x/health')).toEqual({ url: '/clearing-v2x/health', base: '' });
  expect(stripBase('/clearing-v2')).toEqual({ url: '/', base: '/clearing-v2' });
  expect(withBase('<a href="//cdn.x/y">', '/clearing-v2')).toBe('<a href="//cdn.x/y">');
  expect(withBase('<script src="/app.js">', '/clearing-v2')).toBe('<script src="/clearing-v2/app.js">');
});

test('the importmap and inline fetches follow the prefix, so the room key loads under /clearing-v2', () => {
  const html = '<script type="importmap">{"imports":{"@noble/curves/": "/vendor/@noble/curves/"}}</script><script>fetch(\'/api/cards/\' + id)</script>';
  const out = withBase(html, '/clearing-v2');
  expect(out).toContain('"/clearing-v2/vendor/@noble/curves/"');
  expect(out).toContain("fetch('/clearing-v2/api/cards/'");
  expect(withBase(html, '')).toBe(html);
});
