/**
 * chorus-cost handler — unit tests (#2189).
 *
 * Tests verify:
 *   - success → 200 with period, output, partial=false
 *   - stderr present alongside stdout → partial=true (not 500)
 *   - exec fails AND no stdout → 500 with detail
 *   - exec fails BUT has stdout → 200 partial=true (degrades gracefully)
 *   - period passed through to body unchanged
 */
import { fetchCost, type ExecFileFn } from '../../src/handlers/chorus-cost';

function okExec(stdout: string, stderr = ''): ExecFileFn {
  return async () => ({ stdout, stderr });
}

function failExec(stdout = '', stderr = '', msg = 'exit 1'): ExecFileFn {
  return async () => {
    const e = new Error(msg) as Error & { stdout?: string; stderr?: string };
    e.stdout = stdout;
    e.stderr = stderr;
    throw e;
  };
}

describe('fetchCost (#2189 /api/chorus/cost)', () => {
  test('success → 200 with period + output + partial=false', async () => {
    const r = await fetchCost('summary', { execFile: okExec('today: $1.23\n') });
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ period: 'summary', output: 'today: $1.23', partial: false });
  });

  test('stdout + stderr both present → partial=true', async () => {
    const r = await fetchCost('daily', {
      execFile: okExec('data line\n', 'some warning'),
    });
    expect(r.status).toBe(200);
    const b = r.body as { partial: boolean; output: string };
    expect(b.partial).toBe(true);
    expect(b.output).toBe('data line');
  });

  test('exec fails + no stdout → 500 with detail from stderr', async () => {
    const r = await fetchCost('summary', {
      execFile: failExec('', 'script missing'),
    });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'cost-report.sh failed', detail: 'script missing' });
  });

  test('exec fails but has stdout → 200 partial=true', async () => {
    const r = await fetchCost('summary', {
      execFile: failExec('partial result\n', 'some error'),
    });
    expect(r.status).toBe(200);
    const b = r.body as { partial: boolean; output: string };
    expect(b.output).toBe('partial result');
    expect(b.partial).toBe(true);
  });

  test('period passed through unchanged', async () => {
    const r = await fetchCost('weekly', { execFile: okExec('x') });
    const b = r.body as { period: string };
    expect(b.period).toBe('weekly');
  });
});
