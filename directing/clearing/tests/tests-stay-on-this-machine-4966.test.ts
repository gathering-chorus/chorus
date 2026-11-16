// @test-type: integration — a local server on a free port that redirects off this machine; nothing public is ever reached.
// @domain: messages
// @card: #4966
// @owner: abby-normal
/**
 * #4966 AC4 — no test run signs out or replaces a real person's session at the public guard.
 *
 * Measured 2026-11-15: server-unit.test.ts 'GET /logout' let fetch follow the route's redirect
 * to the public guard, so every run of this suite made a sign-out and a sign-in start there (the
 * `ua=node` lines in the guard's log at 03:23, 03:39 and 11:19). It closed nothing of Jeff's,
 * and it is still a test reaching a real person's door.
 *
 * tests/stay-local-4966.setup.ts makes that impossible for every test in this package: a fetch
 * to another machine, or a redirect that leads to one, fails naming the address.
 */
import http from 'http';

const PUBLIC_LOGOUT = 'https://chorus.lightlifeurbangardens.com/_auth/logout';

function redirector(to: string): Promise<{ base: string; close: () => Promise<void> }> {
  const server = http.createServer((req, res) => {
    if (req.url === '/go') { res.writeHead(302, { Location: to }); res.end(); return; }
    res.writeHead(200, { 'Content-Type': 'text/plain' }); res.end('here');
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({
    base: `http://127.0.0.1:${(server.address() as { port: number }).port}`,
    close: () => new Promise<void>((r) => server.close(() => r())),
  })));
}

describe('#4966 tests stay on this machine', () => {
  test('a fetch straight to the public guard fails, naming the address', async () => {
    await expect(fetch(PUBLIC_LOGOUT)).rejects.toThrow(/leave this machine.*chorus\.lightlifeurbangardens\.com\/_auth\/logout/);
  });

  test("the live sign-in guard's own ports on this machine are refused too", async () => {
    await expect(fetch('http://127.0.0.1:8899/_auth/logout')).rejects.toThrow(/leave this machine/);
    await expect(fetch('http://localhost:8898/_auth/login')).rejects.toThrow(/leave this machine/);
  });

  // the shape that reached the guard: a local route that redirects to it
  test('a local redirect that leads to the public guard is not followed', async () => {
    const s = await redirector(PUBLIC_LOGOUT);
    try {
      await expect(fetch(`${s.base}/go`)).rejects.toThrow(/leave this machine.*_auth\/logout/);
    } finally { await s.close(); }
  });

  test('asking not to follow shows the redirect itself, which is what a test should assert', async () => {
    const s = await redirector(PUBLIC_LOGOUT);
    try {
      const res = await fetch(`${s.base}/go`, { redirect: 'manual' });
      expect(res.status).toBe(302);
      expect(res.headers.get('location')).toBe(PUBLIC_LOGOUT);
    } finally { await s.close(); }
  });

  // NEGATIVE PROOF: the guard on fetch does not stop tests talking to this machine
  test('a local fetch, and a local redirect to a local page, work as before', async () => {
    const s = await redirector('/landed');
    try {
      const res = await fetch(`${s.base}/go`);
      expect(res.status).toBe(200);
      expect(await res.text()).toBe('here');
    } finally { await s.close(); }
  });
});
