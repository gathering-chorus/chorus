/**
 * chorus-perf handler — unit tests (#2189).
 *
 * GET /api/chorus/perf parses `perf-baseline.sh summary` output.
 *
 * Tests verify:
 *   - exec failure → 500 with detail from stderr
 *   - parses multiple rows with PASS/FAIL status
 *   - date extracted from "Perf Baseline — <date>" header
 *   - summary line captured from "N/M passed"
 *   - passed count = rows where status=PASS
 *   - empty / no Function header → empty results array
 *   - comma thousands stripped from ms values
 */
import { fetchPerf, type ExecFileFn } from '../../src/handlers/chorus-perf';

function okExec(stdout: string): ExecFileFn {
  return async () => ({ stdout, stderr: '' });
}

function failExec(msg: string, stderr = ''): ExecFileFn {
  return async () => {
    const e = new Error(msg) as Error & { stderr?: string };
    e.stderr = stderr;
    throw e;
  };
}

describe('fetchPerf (#2189 /api/chorus/perf)', () => {
  test('exec failure → 500 with detail from stderr', async () => {
    const r = await fetchPerf({ execFile: failExec('exit 1', 'script not found') });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'perf-baseline.sh failed', detail: 'script not found' });
  });

  test('parses rows with PASS/FAIL + extracts date + summary', async () => {
    const out = [
      'Perf Baseline — 2026-04-18',
      '',
      'Function           Today    Yesterday  Delta           Status',
      '----               -----    ---------  -----           ------',
      'fuseki:ping        100ms    120ms      ▼-17%           PASS',
      'fuseki:graph_count 3,432ms  2,653ms    ▲+29% !         FAIL',
      '',
      '1/2 passed',
    ].join('\n');
    const r = await fetchPerf({ execFile: okExec(out) });
    expect(r.status).toBe(200);
    const b = r.body as { date: string | null; summary: string; passed: number; total: number; results: Array<Record<string, unknown>> };
    expect(b.date).toBe('2026-04-18');
    expect(b.summary).toBe('1/2 passed');
    expect(b.passed).toBe(1);
    expect(b.total).toBe(2);
    expect(b.results).toHaveLength(2);
    expect(b.results[0]).toEqual({
      function: 'fuseki:ping',
      today_ms: 100,
      yesterday_ms: 120,
      delta_pct: '▼-17%',
      status: 'PASS',
    });
    expect(b.results[1].today_ms).toBe(3432); // comma stripped
    expect(b.results[1].status).toBe('FAIL');
  });

  test('empty output → empty results', async () => {
    const r = await fetchPerf({ execFile: okExec('') });
    const b = r.body as { results: unknown[]; passed: number; total: number; date: string | null };
    expect(b.results).toEqual([]);
    expect(b.passed).toBe(0);
    expect(b.total).toBe(0);
    expect(b.date).toBeNull();
  });

  test('no Function header → results empty even with other lines', async () => {
    const out = 'Perf Baseline — 2026-04-18\nsome unrelated output\n1/0 passed';
    const r = await fetchPerf({ execFile: okExec(out) });
    const b = r.body as { results: unknown[]; date: string | null; summary: string };
    expect(b.results).toEqual([]);
    expect(b.date).toBe('2026-04-18');
    expect(b.summary).toBe('1/0 passed');
  });

  test('missing summary line falls back to computed "passed/total"', async () => {
    const out = [
      'Function      Today  Yesterday  Delta  Status',
      '----          -----  ---------  -----  ------',
      'a             50ms   40ms       +25%   PASS',
    ].join('\n');
    const r = await fetchPerf({ execFile: okExec(out) });
    const b = r.body as { summary: string; passed: number; total: number };
    expect(b.summary).toBe('1/1 passed');
    expect(b.passed).toBe(1);
    expect(b.total).toBe(1);
  });
});
