// @test-type: unit — fetchOpenSessionRows against a stubbed fetch; no live door.
// @domain: roles
// @card: #4948
// @owner: wren
import { fetchOpenSessionRows } from './open-sessions';

const NOW = Date.parse('2026-11-10T21:00:00Z');
type Answer = { ok: boolean; status: number; body: unknown };

function stub(answer: (url: string) => Answer): string[] {
  const asked: string[] = [];
  (global as unknown as { fetch: unknown }).fetch = (url: string) => {
    asked.push(url);
    const a = answer(url);
    return Promise.resolve({ ok: a.ok, status: a.status, json: () => Promise.resolve(a.body) });
  };
  return asked;
}

test('#4948 an agent row whose login expiry lapsed hours ago still arrives (the pane query has no expiry filter)', async () => {
  const asked = stub((u) => u.includes('channel=pane')
    ? { ok: true, status: 200, body: { data: [{ name: 'wren-s', channel: 'pane', expiresAt: '2026-11-09T16:32:00Z' }] } }
    : { ok: true, status: 200, body: { data: [{ name: 'jeff-b', channel: 'browser' }] } });
  const rows = await fetchOpenSessionRows('http://door', 2000, NOW);
  expect(rows.map((r) => r.name).sort()).toEqual(['jeff-b', 'wren-s']);
  expect(asked.some((u) => u.includes('channel=pane') && !u.includes('expiresAt'))).toBe(true);
});

test('#4948 a row both queries return is listed once', async () => {
  stub(() => ({ ok: true, status: 200, body: { data: [{ name: 'wren-s', channel: 'pane' }] } }));
  expect((await fetchOpenSessionRows('http://door', 2000, NOW)).length).toBe(1);
});

test('#4948 NEGATIVE PROOF: either query failing is an error, never an empty list', async () => {
  stub((u) => u.includes('channel=pane') ? { ok: false, status: 502, body: {} } : { ok: true, status: 200, body: { data: [] } });
  await expect(fetchOpenSessionRows('http://door', 2000, NOW)).rejects.toThrow(/HTTP 502/);
  stub(() => ({ ok: true, status: 200, body: { error: 'x' } }));
  await expect(fetchOpenSessionRows('http://door', 2000, NOW)).rejects.toThrow(/no data list/);
});
