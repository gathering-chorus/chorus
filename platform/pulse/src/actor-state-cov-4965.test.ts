// @domain: awareness
// @test-type: unit — deriveActorState, readActivePairsFromDisk, ActorStateService, fetchDaguCardSteps and buildLiveActorFactFetcher in actor-state.ts against fixtures, temp files and a stub door on 127.0.0.1; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the actor reads pulse shows: a replied nudge no longer holds its
 * sender waiting; a card title comes from the review or the pair when WIP has
 * none; the pair registry on disk names the driver (from the JSON, else the
 * pair notes) and skips what it cannot read; a dagu run with no active step
 * shows the run's own status; and one live read over every door puts each
 * door's answer, or its failure, where the actor reads it.
 */
import fs from 'fs';
import http from 'http';
import os from 'os';
import path from 'path';
import {
  deriveActorState, readActivePairsFromDisk, ActorStateService, fetchDaguCardSteps, buildLiveActorFactFetcher, type ActorFactInput,
} from './actor-state';

const NOW = new Date('2026-10-16T12:20:00Z');
const live = (principal: string) => ({ id: `s-${principal}`, principal, channel: 'pane', expiresAt: '2026-10-16T13:00:00Z', lastSeenAt: '2026-10-16T12:18:00Z' });

describe('#4965 deriveActorState', () => {
  test('a nudge already replied to does not hold its sender waiting', () => {
    const base: ActorFactInput = {
      now: NOW, principals: ['principal-silas'], sessions: [live('principal-silas')],
      actions: { silas: { lastActionAt: '2026-10-16T12:19:00Z', recent5mCount: 1 } },
      owedNudges: [{ from: 'silas', to: 'kade', expects: 'reply', sentAt: '2026-10-16 12:15:00' }],
    };
    expect(deriveActorState(base)[0].stateValue).toBe('waiting');
    const replied = { ...base, owedNudges: [{ ...base.owedNudges![0], repliedAt: '2026-10-16T12:16:00Z' }] };
    expect(deriveActorState(replied)[0].stateValue).not.toBe('waiting');
    expect(deriveActorState(replied)[0].waitingOn).toBeUndefined();
  });

  test('negative: a reply stamped before the nudge does not clear it', () => {
    const input: ActorFactInput = {
      now: NOW, principals: ['principal-silas'], sessions: [live('principal-silas')],
      owedNudges: [{ from: 'silas', to: 'kade', expects: 'reply', sentAt: '2026-10-16T12:15:00Z', repliedAt: '2026-10-16T12:10:00Z' }],
    };
    expect(deriveActorState(input)[0].stateValue).toBe('waiting');
  });

  test('a reviewed card takes its title from the review row', () => {
    const [jeff] = deriveActorState({
      now: NOW, principals: [{ principal: 'principal-jeff', roleKind: 'human' }],
      sessions: [{ ...live('principal-jeff'), channel: 'browser' }],
      actions: { jeff: { lastActionAt: '2026-10-16T12:18:00Z', recent5mCount: 2 } },
      reviewCards: [{ cardId: 4560, reviewer: 'jeff', peer: 'kade', title: 'Review title' }],
    });
    expect(jeff.currentCard).toBe(4560);
    expect(jeff.currentCardTitle).toBe('Review title');
  });

  test('a paired card with an untitled WIP row takes its title from the pair', () => {
    const states = deriveActorState({
      now: NOW, principals: ['principal-wren', 'principal-kade'],
      sessions: [live('principal-wren'), live('principal-kade')],
      activePairs: [{ card: 4595, navigator: 'wren', driver: 'kade', title: 'Pair title' }],
      wipCards: [{ cardId: 4595, owner: 'kade' }],
    });
    const wren = states.find((s) => s.aboutPrincipal === 'principal-wren')!;
    expect(wren.currentCard).toBe(4595);
    expect(wren.currentCardTitle).toBe('Pair title');
  });
});

describe('#4965 readActivePairsFromDisk', () => {
  let dir: string;
  const cards = [99654961, 99654962, 99654963, 99654964];
  const md = (c: number) => `/tmp/pair-${c}.md`; // the module reads the pair notes at this fixed path
  beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pairs-4965-')); });
  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
    for (const c of cards) fs.rmSync(md(c), { recursive: true, force: true });
  });

  test('the driver comes from the JSON, else from the pair notes; unreadable entries are skipped', () => {
    fs.writeFileSync(path.join(dir, `${cards[0]}.json`), JSON.stringify({ navigator: 'wren', driver: 'kade' }));
    fs.writeFileSync(path.join(dir, `${cards[1]}.json`), JSON.stringify({ navigator: 'silas' }));
    fs.writeFileSync(md(cards[1]), '# pair\nDriver: Abby-Normal\n');
    fs.writeFileSync(path.join(dir, `${cards[2]}.json`), '{ not json');
    fs.mkdirSync(md(cards[2])); // notes path is a directory: unreadable, no driver
    fs.mkdirSync(path.join(dir, `${cards[3]}.json`)); // a directory named like a pair: unreadable, skipped
    fs.writeFileSync(path.join(dir, 'notes.txt'), 'x');
    fs.writeFileSync(path.join(dir, 'abc.json'), '{}');
    const pairs = readActivePairsFromDisk(dir).sort((a, b) => a.card - b.card);
    expect(pairs).toEqual([
      { card: cards[0], navigator: 'wren', driver: 'kade' },
      { card: cards[1], navigator: 'silas', driver: 'abby-normal' },
      { card: cards[2], navigator: '', driver: undefined },
    ]);
  });

  test('negative: a registry path that is a file, not a directory, reads as no pairs', () => {
    const file = path.join(dir, 'not-a-dir');
    fs.writeFileSync(file, 'x');
    expect(readActivePairsFromDisk(file)).toEqual([]);
  });
});

describe('#4965 ActorStateService polls on its period', () => {
  test('start twice runs one poll loop that re-reads the facts each period', async () => {
    let reads = 0;
    const svc = new ActorStateService(async () => { reads += 1; return { now: NOW, principals: ['principal-kade'] }; }, 15);
    svc.start();
    svc.start();
    await new Promise((r) => setTimeout(r, 70));
    svc.stop();
    const after = reads;
    expect(after).toBeGreaterThanOrEqual(3);
    expect(after).toBeLessThan(10); // a second loop would have doubled the reads
    expect(svc.getActorStates().actors.map((a) => a.aboutPrincipal)).toEqual(['principal-kade']);
  });
});

type Route = (url: string) => { status: number; body: unknown } | undefined;
const door = (route: Route) => new Promise<{ url: string; hits: string[]; close: () => Promise<void> }>((resolve) => {
  const hits: string[] = [];
  const s = http.createServer((req, res) => {
    hits.push(req.url ?? '');
    const r = route(req.url ?? '') ?? { status: 404, body: {} };
    res.statusCode = r.status;
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify(r.body));
  });
  s.listen(0, '127.0.0.1', () => resolve({
    url: `http://127.0.0.1:${(s.address() as { port: number }).port}`,
    hits,
    close: () => new Promise<void>((r) => s.close(() => r())),
  }));
});

describe('#4965 fetchDaguCardSteps with no active step', () => {
  test('a run with no running/waiting/failed node shows the run status, or "coding" with none', async () => {
    const d = await door((u) => {
      if (u.startsWith('/api/v1/dag-runs')) return { status: 200, body: { dagRuns: [
        { params: 'CARD=1', dagRunId: 'r1', statusLabel: 'queued' },
        { params: 'CARD=2', dagRunId: 'r2' },
      ] } };
      if (u.includes('/dag-runs/r1') || u.includes('/dag-runs/r2')) return { status: 200, body: { dagRun: { nodes: [{ step: { name: 'werk-build' }, statusLabel: 'succeeded' }] } } };
      return undefined;
    });
    try {
      const unmeasured: Array<{ source: string; reason: string }> = [];
      const steps = await fetchDaguCardSteps(d.url, [{ cardId: 1, owner: 'kade' }, { cardId: 2, owner: 'kade' }], unmeasured);
      expect(steps).toEqual({ 1: 'queued', 2: 'coding' });
      expect(unmeasured).toEqual([]);
    } finally { await d.close(); }
  });

  test('negative: a run detail that is not JSON reads unmeasured, never a step', async () => {
    const s = http.createServer((req, res) => {
      res.setHeader('content-type', 'application/json');
      if ((req.url ?? '').startsWith('/api/v1/dag-runs')) res.end(JSON.stringify({ dagRuns: [{ params: 'CARD=3', dagRunId: 'r3', statusLabel: 'running' }] }));
      else res.end('<html>proxy error</html>');
    });
    await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
    try {
      const url = `http://127.0.0.1:${(s.address() as { port: number }).port}`;
      expect(await fetchDaguCardSteps(url, [{ cardId: 3, owner: 'kade' }], [])).toEqual({ 3: 'unmeasured' });
    } finally { await new Promise<void>((r) => s.close(() => r())); }
  });
});

describe('#4965 one live read over every door', () => {
  test('each door answer lands where the actor reads it; the ones that fail read unmeasured', async () => {
    const d = await door((u) => {
      if (u.startsWith('/v1/roles/roles')) return { status: 200, body: { data: [
        { name: 'kade', label: 'role-kade', roleKind: 'agent' },
        { name: 'cron', roleKind: 'automation' },
      ] } };
      if (u.startsWith('/v1/identity/sessions')) return { status: 200, body: { data: [] } };
      if (u.startsWith('/v1/identity/presences') || u.startsWith('/v1/identity/sessionruns')) return { status: 200, body: { data: [] } };
      if (u.startsWith('/api/chorus/context/board/wip')) return { status: 200, body: { data: { cards: [{ id: 4965, title: 'Coverage', owner: 'Kade' }] } } };
      if (u.startsWith('/api/v1/dag-runs')) return { status: 200, body: { dagRuns: [{ params: 'CARD=4965', dagRunId: 'run-a', statusLabel: 'running' }] } };
      if (u.startsWith('/api/v1/dags/cicd/dag-runs/run-a')) return { status: 503, body: {} };
      return undefined;
    });
    try {
      const fetcher = buildLiveActorFactFetcher({
        store: { getOwedNudges: () => [] },
        readTail: () => Promise.reject(new Error('spine unreadable')),
        athenaBaseUrl: d.url, chorusApiBaseUrl: d.url, lokiBaseUrl: 'off', daguBaseUrl: d.url,
        opsDir: path.join(os.tmpdir(), 'no-such-pairs-4965'),
      });
      const input = await fetcher();
      expect(input.principals).toEqual([{ principal: 'principal-kade', name: 'Kade', roleKind: 'agent' }]);
      expect(input.wipCards).toEqual([{ cardId: 4965, title: 'Coverage', owner: 'kade' }]);
      // negative: the run detail answered 503, so the step is unmeasured, never a guessed step
      expect(input.cardSteps).toEqual({ 4965: 'unmeasured' });
      // presences read with no live presence for kade: offline, said as a fact
      expect(input.reachability).toEqual({ kade: 'offline' });
      // a spine tail that cannot be read adds no actions and does not fail the read
      expect(input.actions).toEqual({});
      expect(input.activePairs).toEqual([]);
      expect(input.unmeasured).toEqual([{ source: 'loki:activity', reason: 'loki off in this environment' }]);
    } finally { await d.close(); }
  });

  test('negative: a presences door with no data list reads unmeasured and gives no reachability', async () => {
    const d = await door((u) => {
      if (u.startsWith('/v1/roles/roles')) return { status: 200, body: { data: [{ name: 'kade', roleKind: 'agent' }] } };
      if (u.startsWith('/v1/identity/sessions')) return { status: 200, body: { data: [] } };
      if (u.startsWith('/v1/identity/presences') || u.startsWith('/v1/identity/sessionruns')) return { status: 200, body: { nope: true } };
      if (u.startsWith('/api/chorus/context/board/wip')) return { status: 200, body: { data: { cards: [] } } };
      return undefined;
    });
    try {
      const input = await buildLiveActorFactFetcher({
        store: { getOwedNudges: () => [] }, readTail: () => Promise.resolve(''),
        athenaBaseUrl: d.url, chorusApiBaseUrl: d.url, lokiBaseUrl: 'off', daguBaseUrl: d.url,
        opsDir: path.join(os.tmpdir(), 'no-such-pairs-4965'),
      })();
      expect(input.reachability).toBeUndefined();
      expect(input.unmeasured).toContainEqual({ source: 'athena:identity:presences', reason: expect.stringMatching(/answered no data list/) });
    } finally { await d.close(); }
  });
});
