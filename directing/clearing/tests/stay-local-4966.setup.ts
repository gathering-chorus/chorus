/**
 * #4966 AC4 — tests stay on this machine.
 *
 * Every test in this package gets a fetch that refuses to reach another machine, directly or
 * by following a redirect. Measured 2026-11-15: one test followed a local route's redirect to
 * the public sign-in guard on every run. A test asserts on the redirect (redirect: 'manual');
 * it never follows one off this machine.
 */
const LOCAL = new Set(['127.0.0.1', 'localhost', '[::1]', '::1']);
const realFetch = globalThis.fetch;

// the live sign-in guards listen on this machine too (chorus-share-guard, :8899 and :8898):
// reaching them from a test is reaching a real person's door (Abby 2026-11-15)
const LIVE_GUARD_PORTS = new Set(['8899', '8898']);

function isLocal(u: URL): boolean {
  return LOCAL.has(u.hostname) && !LIVE_GUARD_PORTS.has(u.port);
}

function left(u: URL): Error {
  return new Error(`#4966: this test would leave this machine: ${u.origin}${u.pathname}. Assert on the redirect (redirect: 'manual') or stub the address.`);
}

globalThis.fetch = (async (input: Parameters<typeof fetch>[0], init?: Parameters<typeof fetch>[1]) => {
  let url = new URL(typeof input === 'string' || input instanceof URL ? String(input) : (input as Request).url);
  if (!isLocal(url)) throw left(url);
  const mode = init?.redirect ?? (input instanceof Request ? input.redirect : 'follow');
  if (mode !== 'follow') return realFetch(input, init);
  // follow by hand, so every hop is checked before it is made
  let res = await realFetch(input, { ...init, redirect: 'manual' });
  for (let hops = 0; hops < 10 && res.status >= 300 && res.status < 400 && res.headers.get('location'); hops++) {
    url = new URL(res.headers.get('location') as string, url);
    if (!isLocal(url)) throw left(url);
    const keepBody = res.status === 307 || res.status === 308;
    res = await realFetch(url, { ...init, method: keepBody ? init?.method : 'GET', body: keepBody ? init?.body : undefined, redirect: 'manual' });
  }
  return res;
}) as typeof fetch;
