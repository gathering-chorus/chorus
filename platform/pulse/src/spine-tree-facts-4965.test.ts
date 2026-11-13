// @domain: awareness
// @test-type: integration — SpineTreeFacts in platform/pulse/src/pulse-tree.ts against a local stub Loki; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the pulse tree reads its turn and run lines from Loki, and nothing tested the
 * reader. A good answer replaces the lines; a bad one keeps the last good lines and says
 * why (unmeasured), so the tree never goes blank on a Loki hiccup and never reads stale
 * lines as fresh without saying so.
 */
import * as http from 'http';
import { SpineTreeFacts, SPINE_TREE_QUERY } from './pulse-tree';

const loki = (answer: () => { status: number; body: unknown }) =>
  new Promise<{ url: string; seen: string[]; close: () => Promise<void> }>((resolve) => {
    const seen: string[] = [];
    const s = http.createServer((req, res) => {
      seen.push(req.url ?? '');
      const a = answer();
      res.statusCode = a.status;
      res.setHeader('content-type', 'application/json');
      res.end(typeof a.body === 'string' ? a.body : JSON.stringify(a.body));
    });
    s.listen(0, '127.0.0.1', () => resolve({
      url: `http://127.0.0.1:${(s.address() as { port: number }).port}`,
      seen,
      close: () => new Promise<void>((r) => s.close(() => r())),
    }));
  });

const stream = (...lines: string[]) => ({ data: { result: [{ values: lines.map((l, i) => [String(i), l]) }] } });

describe('#4965 SpineTreeFacts — the tree\'s spine lines from Loki', () => {
  it('asks Loki for the tree query and keeps every spine line, skipping what is not JSON', async () => {
    const l = await loki(() => ({ status: 200, body: stream('{"event":"agent.turn.started","role":"kade"}', 'not json', '{"event":"pipeline.run.started"}') }));
    try {
      const f = new SpineTreeFacts(l.url);
      await f.refresh();
      expect(decodeURIComponent(l.seen[0])).toContain(SPINE_TREE_QUERY);
      expect(f.get()).toEqual([{ event: 'agent.turn.started', role: 'kade' }, { event: 'pipeline.run.started' }]);
      expect(f.unmeasured).toBeUndefined();
    } finally { await l.close(); }
  });

  it('NEGATIVE: a Loki error keeps the last good lines and says why; the next good read clears it', async () => {
    let status = 200;
    const l = await loki(() => (status === 200 ? { status, body: stream('{"event":"agent.turn.ended"}') } : { status, body: {} }));
    try {
      const f = new SpineTreeFacts(l.url);
      await f.refresh();
      status = 502;
      await f.refresh();
      expect(f.unmeasured).toBe('loki HTTP 502');
      expect(f.get()).toEqual([{ event: 'agent.turn.ended' }]);   // kept, not blanked
      status = 200;
      await f.refresh();
      expect(f.unmeasured).toBeUndefined();
    } finally { await l.close(); }
  });

  it('NEGATIVE: Loki not answering is unmeasured with the reason, never an empty "nothing happened"', async () => {
    const f = new SpineTreeFacts('http://127.0.0.1:9');
    await f.refresh();
    expect(f.unmeasured).toMatch(/^loki: /);
    expect(f.get()).toEqual([]);
  });
});
