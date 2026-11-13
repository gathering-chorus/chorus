// @domain: awareness
// @test-type: integration — fetchDaguCardSteps in platform/pulse/src/actor-state.ts against a local stub dagu; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the step pulse shows for each WIP card comes from dagu's cicd runs, and nothing
 * tested the reader: which run is the card's, which node is its step, and what an answer
 * dagu cannot give reads as. Each step label is shown, and every failure reads
 * "unmeasured", never a step it did not see.
 */
import * as http from 'http';
import { fetchDaguCardSteps, type WipCardFact } from './actor-state';

type Route = (url: string) => { status: number; body: unknown } | undefined;

const dagu = (route: Route) => new Promise<{ url: string; close: () => Promise<void> }>((resolve) => {
  const s = http.createServer((req, res) => {
    const r = route(req.url ?? '') ?? { status: 404, body: {} };
    res.statusCode = r.status;
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify(r.body));
  });
  s.listen(0, '127.0.0.1', () => resolve({
    url: `http://127.0.0.1:${(s.address() as { port: number }).port}`,
    close: () => new Promise<void>((r) => s.close(() => r())),
  }));
});

const card = (cardId: number): WipCardFact => ({ cardId, owner: 'kade' });
const node = (name: string, statusLabel: string) => ({ step: { name }, statusLabel });

describe('#4965 fetchDaguCardSteps — the step each WIP card is at', () => {
  it('no WIP cards asks dagu nothing', async () => {
    const unmeasured: Array<{ source: string; reason: string }> = [];
    expect(await fetchDaguCardSteps('http://127.0.0.1:9', [], unmeasured)).toEqual({});
    expect(unmeasured).toEqual([]);
  });

  it('names the step of the newest run for each card: running, waiting, failed, landed, coding', async () => {
    const runs = [
      { dagRunId: 'old', params: 'CARD=1 ROLE=kade', statusLabel: 'failed', queuedAt: '2026-11-13T01:00:00Z' },
      { dagRunId: 'r1', params: 'CARD=1 ROLE=kade', statusLabel: 'running', queuedAt: '2026-11-13T02:00:00Z' },
      { dagRunId: 'r2', params: 'CARD="2" ROLE=wren', statusLabel: 'running', startedAt: '2026-11-13T02:00:00Z' },
      { dagRunId: 'r3', params: 'CARD=3', statusLabel: 'failed', queuedAt: '2026-11-13T02:00:00Z' },
      { dagRunId: 'r4', params: 'CARD=4', statusLabel: 'succeeded', queuedAt: '2026-11-13T02:00:00Z' },
      { dagRunId: 'r12', params: 'CARD=12', statusLabel: 'running', queuedAt: '2026-11-13T03:00:00Z' },
    ];
    const details: Record<string, unknown> = {
      r1: { dagRun: { nodes: [node('skill-werk-build', 'succeeded'), node('skill-werk-test-pipeline', 'running')] } },
      r2: { dagRun: { nodes: [node('cicd-go', 'waiting')] } },
      r3: { dagRun: { nodes: [node('skill-werk-deploy-demo', 'failed')] } },
      r4: { dagRun: { nodes: [node('skill-werk-accept', 'succeeded')] } },
    };
    const d = await dagu((u) => {
      if (u.startsWith('/api/v1/dag-runs?name=cicd')) return { status: 200, body: { dagRuns: runs } };
      const m = u.match(/^\/api\/v1\/dags\/cicd\/dag-runs\/(\w+)$/);
      return m && details[m[1]] ? { status: 200, body: details[m[1]] } : undefined;
    });
    try {
      const unmeasured: Array<{ source: string; reason: string }> = [];
      const steps = await fetchDaguCardSteps(d.url, [1, 2, 3, 4, 5].map(card), unmeasured);
      expect(steps).toEqual({
        1: 'werk-test',              // the newest run, not the older failed one; skill- and -pipeline cut
        2: 'cicd-go',                // waiting at the go
        3: 'failed at werk-deploy-demo',
        4: 'landed',                 // no active node, the run succeeded
        5: 'coding',                 // no run names card 5 (CARD=12 must not match it)
      });
      expect(unmeasured).toEqual([]);
    } finally { await d.close(); }
  });

  it('NEGATIVE: a run list dagu will not give is unmeasured for every card, and says why', async () => {
    const d = await dagu(() => ({ status: 503, body: {} }));
    try {
      const unmeasured: Array<{ source: string; reason: string }> = [];
      expect(await fetchDaguCardSteps(d.url, [card(1), card(2)], unmeasured)).toEqual({ 1: 'unmeasured', 2: 'unmeasured' });
      expect(unmeasured).toEqual([{ source: 'dagu:runs', reason: 'HTTP 503' }]);
    } finally { await d.close(); }
  });

  it('NEGATIVE: a run whose detail dagu will not give is unmeasured, never a guessed step', async () => {
    const d = await dagu((u) => (u.startsWith('/api/v1/dag-runs?name=cicd')
      ? { status: 200, body: { dagRuns: [{ dagRunId: 'gone', params: 'CARD=7', statusLabel: 'running' }] } }
      : { status: 500, body: {} }));
    try {
      const unmeasured: Array<{ source: string; reason: string }> = [];
      expect(await fetchDaguCardSteps(d.url, [card(7)], unmeasured)).toEqual({ 7: 'unmeasured' });
    } finally { await d.close(); }
  });

  it('NEGATIVE: dagu not answering at all is unmeasured for every card, with the reason', async () => {
    const unmeasured: Array<{ source: string; reason: string }> = [];
    expect(await fetchDaguCardSteps('http://127.0.0.1:9', [card(8)], unmeasured)).toEqual({ 8: 'unmeasured' });
    expect(unmeasured[0].source).toBe('dagu:runs');
  });
});
