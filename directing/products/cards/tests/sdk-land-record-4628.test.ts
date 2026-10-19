// @test-type: unit — a fake fetch stands in for pulse; no network.
// @domain: cards
// @card: #4628
// @owner: silas
/**
 * #4628 — `cards done` posts the land notice to pulse's record route, so it
 * reaches Buzz once from one writer. Until 10-19 only Clearing v1's spine
 * tailer posted it, with no author (13 that day). The author is the land's
 * accepter (ACCEPTER, from the go step), the card owner a tag.
 */
import { recordLand, type FetchLike } from '../src/sdk';

type Call = { url: string; headers: Record<string, string>; body: Record<string, unknown> };
const fake = (status: number, calls: Call[]): FetchLike => async (url, init) => {
  calls.push({ url, headers: init?.headers ?? {}, body: JSON.parse(init?.body ?? '{}') });
  return { ok: status < 400, status, text: async (): Promise<string> => (status === 422 ? '{"error":"no-accepter"}' : '{}') };
};

describe('#4628 cards done records the land in Buzz through pulse', () => {
  test('posts kind land with the card, title, owner and accepter, carrying the pulse secret', async () => {
    const calls: Call[] = [];
    const r = await recordLand({ card: 4630, title: 'Backups', owner: 'Silas', accepter: 'jeff', secret: 's3', fetchImpl: fake(200, calls) });
    expect(r.recorded).toBe(true);
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe('http://localhost:3475/api/record');
    expect(calls[0].body).toEqual({ kind: 'land', card: 4630, title: 'Backups', owner: 'silas', accepter: 'jeff' });
    expect(calls[0].headers['X-Chorus-Pulse-Secret']).toBe('s3');
  });

  // NEGATIVE PROOF: pulse refuses a land with no accepter; the refusal is reported, never swallowed as recorded.
  test('negative proof: a refused land (no accepter) comes back as not recorded, with the reason', async () => {
    const calls: Call[] = [];
    const r = await recordLand({ card: 4568, title: 't', owner: 'Kade', accepter: undefined, secret: 's3', fetchImpl: fake(422, calls) });
    expect(r.recorded).toBe(false);
    expect(r.error).toContain('no-accepter');
  });

  test('a pulse that does not answer is a miss with the reason, never a throw', async () => {
    const r = await recordLand({ card: 1, title: '', owner: '', accepter: 'jeff', secret: 's3', fetchImpl: async () => { throw new Error('connect ECONNREFUSED'); } });
    expect(r).toEqual({ recorded: false, error: 'connect ECONNREFUSED' });
  });

  // NEGATIVE PROOF: a test that drives `cards done` without handing it a pulse never reaches live pulse.
  test('negative proof: under jest with no pulse given, nothing is posted anywhere', async () => {
    const realFetch = globalThis.fetch;
    const spy = jest.fn();
    (globalThis as unknown as { fetch: unknown }).fetch = spy;
    try {
      const r = await recordLand({ card: 2692, title: 't', owner: 'silas', accepter: 'jeff' });
      expect(r).toEqual({ recorded: false, error: 'test context: no live pulse' });
      expect(spy).not.toHaveBeenCalled();
    } finally { (globalThis as unknown as { fetch: unknown }).fetch = realFetch; }
  });
});
