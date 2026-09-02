// @test-type: unit — signal:api is fixture-data (in-process harness on an ephemeral port, no live :3340)
// #4060 — the readout is SERVED, so the nudge, the page and any role read one
// source. Hermetic: the in-process harness on an ephemeral port, the log is a
// fixture file the test owns (NIGHTLY_LOG_PATH seam, #3528 — never ~/Library).
import fs from 'fs';
import os from 'os';
import path from 'path';

const LOG = [
  'RUN|start|2026-09-01T03:00:05|pid=1',
  'SUITE|cargo|platform/services/chorus-hooks|silas|fail|30 pass, 2 fail',
  'SUITE|bats|platform/tests/a.bats|kade|pass|bats: 1 passed, 0 failed',
  'RUN|complete|2026-09-01T04:00:05|suites=2',
  'RUN|start|2026-09-02T03:00:05|pid=2',
  'SUITE|cargo|platform/services/chorus-hooks|silas|pass|32 pass, 0 fail',
  'SUITE|bats|platform/tests/a.bats|kade|fail|bats: 0 passed, 1 failed',
  'RUN|complete|2026-09-02T03:30:05|suites=2',
  '',
].join('\n');

let app: import('./lib/test-app').TestApp;
beforeAll(async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nightly-4060-'));
  process.env.NIGHTLY_LOG_PATH = path.join(dir, 'nightly-suites.log');
  fs.writeFileSync(process.env.NIGHTLY_LOG_PATH, LOG);
  const { startTestApp } = await import('./lib/test-app');
  app = await startTestApp();
});
afterAll(async () => { await app.close(); });

describe('GET /api/chorus/nightly/runs', () => {
  it('lists every recorded run, newest first, with its verdict', async () => {
    const r = await fetch(`${app.baseUrl}/api/chorus/nightly/runs`);
    expect(r.status).toBe(200);
    const body = await r.json() as { runs: { runId: string; failed: number }[] };
    expect(body.runs.map((x) => x.runId)).toEqual(['2026-09-02T03:00:05', '2026-09-01T03:00:05']);
    expect(body.runs[1].failed).toBe(1);
  });
});

describe('GET /api/chorus/nightly/runs/:id', () => {
  it('latest is the newest run, with the delta against the one before', async () => {
    const r = await fetch(`${app.baseUrl}/api/chorus/nightly/runs/latest`);
    const j = await r.json() as { runId: string; durationMin: number; failed: number; changes: { newlyRed: { suite: string }[]; fixed: { suite: string }[] } };
    expect(j.runId).toBe('2026-09-02T03:00:05');
    expect(j.durationMin).toBe(30);
    expect(j.failed).toBe(1);
    expect(j.changes.newlyRed.map((x) => x.suite)).toEqual(['platform/tests/a.bats']);
    expect(j.changes.fixed.map((x) => x.suite)).toEqual(['platform/services/chorus-hooks']);
  });

  it('a past run is reachable by id and carries ITS numbers', async () => {
    const r = await fetch(`${app.baseUrl}/api/chorus/nightly/runs/2026-09-01T03:00:05`);
    const j = await r.json() as { runId: string; durationMin: number; reds: { suite: string }[] };
    expect(j.runId).toBe('2026-09-01T03:00:05');
    expect(j.durationMin).toBe(60);
    expect(j.reds.map((x) => x.suite)).toEqual(['platform/services/chorus-hooks']);
  });

  it('text form is the same numbers as the JSON (one source, two renderings)', async () => {
    const t = await (await fetch(`${app.baseUrl}/api/chorus/nightly/runs/latest?format=text`)).text();
    expect(t).toContain('30 min');
    expect(t).toContain('2 suites, 1 red');
    expect(t).toContain('kade   platform/tests/a.bats');
    expect(t).toContain('1 new red, 1 fixed');
    expect(t).toContain('/nightly?run=2026-09-02T03:00:05');
  });

  it('NEGATIVE PROOF (#3734): an unknown run id is 404, never another run\'s numbers', async () => {
    const r = await fetch(`${app.baseUrl}/api/chorus/nightly/runs/2026-08-01T03:00:05`);
    expect(r.status).toBe(404);
  });
});

describe('GET /nightly?run=', () => {
  it('renders the requested past run with the history list', async () => {
    const html = await (await fetch(`${app.baseUrl}/nightly?run=2026-09-01T03:00:05`)).text();
    expect(html).toContain('1 RED SUITES');
    expect(html).toContain('30 pass, 2 fail');
    expect(html).toContain('href="/nightly?run=2026-09-02T03:00:05"');
    expect(html).toContain('took 60 min');
  });
  it('an unknown run id is 404, not the latest run', async () => {
    const r = await fetch(`${app.baseUrl}/nightly?run=2026-08-01T03:00:05`);
    expect(r.status).toBe(404);
  });
});
