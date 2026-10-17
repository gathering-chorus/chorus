// @test-type: unit — a stub fetch stands in for pulse; no live services
// @domain: messages
// @card: #4571
// @owner: wren
// #4571 — Jeff's message from v2 reaches the role through pulse's Jeff-input door, and a
// refused hand-off is reported instead of swallowed (Abby's 20:08 catch: /api/nudge 403'd silently).
import { deliverJeffInput, FetchLike } from '../src/jeff-input';

test('posts to /api/jeff-input as the Clearing caller', async () => {
  const calls: Array<{ url: string; headers: Record<string, string>; body: string }> = [];
  const f: FetchLike = async (url, init) => { calls.push({ url, headers: init.headers, body: init.body }); return { ok: true, status: 202, text: async () => '' }; };
  expect(await deliverJeffInput('http://pulse', 'kade', 'hi @kade', f)).toEqual({ ok: true });
  expect(calls[0].url).toBe('http://pulse/api/jeff-input');
  expect(calls[0].headers['X-Chorus-Clearing-Caller']).toBe('1');
  expect(JSON.parse(calls[0].body)).toEqual({ to: 'kade', content: 'hi @kade' });
});

test('negative proof: a 403 from pulse comes back as a named failure, not success', async () => {
  const f: FetchLike = async () => ({ ok: false, status: 403, text: async () => '{"error":"not-canonical-caller"}' });
  const out = await deliverJeffInput('http://pulse', 'kade', 'x', f);
  expect(out).toEqual({ ok: false, reason: expect.stringContaining('pulse 403') });
});
