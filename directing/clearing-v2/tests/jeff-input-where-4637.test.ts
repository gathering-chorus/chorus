// @test-type: unit — a stub fetch records the hand-off body; no pulse, no relay
// @card: #4637
// @owner: silas
// @domain: messages
/**
 * #4637 AC3 — src/server.ts whereFrom and src/jeff-input.ts: Jeff's Clearing v2
 * post reaches pulse saying where he wrote it (phone or desktop), so pulse's
 * row carries device/app.
 */
import { whereFrom } from '../src/server';
import { deliverJeffInput, FetchLike } from '../src/jeff-input';

test('the browser says phone or desktop', () => {
  expect(whereFrom('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Mobile/15E148')).toBe('phone/clearing-v2');
  expect(whereFrom('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/130')).toBe('desktop/clearing-v2');
  // NEGATIVE PROOF: no user agent is unknown, never guessed as desktop
  expect(whereFrom(undefined)).toBe('unknown/clearing-v2');
});

test('the hand-off to pulse carries where, and leaves it out when unknown to the caller', async () => {
  const bodies: Array<Record<string, unknown>> = [];
  const fetchFn: FetchLike = async (_url, init) => { bodies.push(JSON.parse(init.body)); return { ok: true, status: 200, text: async () => '' }; };
  await deliverJeffInput('http://pulse', 'silas', 'hi', fetchFn, 'jeff-x-silas', 'phone/clearing-v2');
  await deliverJeffInput('http://pulse', 'silas', 'hi', fetchFn, 'jeff-y-silas');
  expect(bodies[0]).toEqual({ to: 'silas', content: 'hi', traceId: 'jeff-x-silas', where: 'phone/clearing-v2' });
  expect(bodies[1]).toEqual({ to: 'silas', content: 'hi', traceId: 'jeff-y-silas' });
});
