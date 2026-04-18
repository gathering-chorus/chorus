/**
 * chorus-services handler — unit tests (#2189).
 *
 * GET /api/chorus/services enumerates com.chorus.* and com.gathering.*
 * LaunchAgents with PID + status + RSS.
 *
 * Tests verify:
 *   - launchctl failure → 500
 *   - only matching label prefixes included (non-prefix entries filtered out)
 *   - PID='-' → null; integer PID parsed
 *   - no running services → running=0, rss_mb=null for all, no ps call
 *   - running service → rss_mb populated from ps output (KB → MB round)
 *   - ps failure → rss_mb=null for all, no 500 (degrades silently)
 *   - total_rss_mb sums rss_mb values
 */
import { fetchServices, type ExecFileFn } from '../../src/handlers/chorus-services';

function mockExec(outputs: Record<string, { stdout?: string; reject?: Error }>): ExecFileFn {
  return async (cmd: string) => {
    const spec = outputs[cmd];
    if (!spec) throw new Error(`unexpected exec: ${cmd}`);
    if (spec.reject) throw spec.reject;
    return { stdout: spec.stdout ?? '', stderr: '' };
  };
}

const LC_HEADER = 'PID\tStatus\tLabel';

describe('fetchServices (#2189 /api/chorus/services)', () => {
  test('launchctl failure returns 500', async () => {
    const r = await fetchServices({
      execFile: mockExec({ launchctl: { reject: new Error('denied') } }),
    });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'launchctl list failed' });
  });

  test('filters to com.chorus.* and com.gathering.* only', async () => {
    const lc = [
      LC_HEADER,
      '1234\t0\tcom.chorus.harvester',
      '-\t0\tcom.apple.Safari',
      '5678\t0\tcom.gathering.fuseki',
      '9999\t0\tcom.foo.bar',
    ].join('\n');
    const r = await fetchServices({
      execFile: mockExec({
        launchctl: { stdout: lc },
        ps: { stdout: '1234   102400\n5678   204800' },
      }),
    });
    expect(r.status).toBe(200);
    const b = r.body as { services: Array<{ label: string }>; total: number };
    expect(b.total).toBe(2);
    expect(b.services.map((s) => s.label)).toEqual(['com.chorus.harvester', 'com.gathering.fuseki']);
  });

  test('PID "-" → null; integer PID parsed', async () => {
    const lc = [
      LC_HEADER,
      '-\t0\tcom.chorus.stopped',
      '1234\t0\tcom.chorus.running',
    ].join('\n');
    const r = await fetchServices({
      execFile: mockExec({
        launchctl: { stdout: lc },
        ps: { stdout: '1234   51200' },
      }),
    });
    const b = r.body as { services: Array<{ label: string; pid: number | null }> };
    expect(b.services[0]).toMatchObject({ label: 'com.chorus.stopped', pid: null });
    expect(b.services[1]).toMatchObject({ label: 'com.chorus.running', pid: 1234 });
  });

  test('no running services → running=0, no ps call needed', async () => {
    const lc = [LC_HEADER, '-\t0\tcom.chorus.a', '-\t0\tcom.chorus.b'].join('\n');
    const r = await fetchServices({
      execFile: mockExec({ launchctl: { stdout: lc } }),
    });
    const b = r.body as { running: number; total: number; services: Array<{ rss_mb: number | null }> };
    expect(b.running).toBe(0);
    expect(b.total).toBe(2);
    expect(b.services.every((s) => s.rss_mb === null)).toBe(true);
  });

  test('running service → rss_mb populated from ps (KB → MB rounded)', async () => {
    const lc = [LC_HEADER, '1234\t0\tcom.chorus.x'].join('\n');
    const r = await fetchServices({
      execFile: mockExec({
        launchctl: { stdout: lc },
        ps: { stdout: '1234   102400' }, // 102400 KB → 100 MB
      }),
    });
    const b = r.body as { services: Array<{ rss_mb: number | null }>; total_rss_mb: number };
    expect(b.services[0].rss_mb).toBe(100);
    expect(b.total_rss_mb).toBe(100);
  });

  test('ps failure → rss_mb=null, no 500', async () => {
    const lc = [LC_HEADER, '1234\t0\tcom.chorus.x'].join('\n');
    const r = await fetchServices({
      execFile: mockExec({
        launchctl: { stdout: lc },
        ps: { reject: new Error('ps not found') },
      }),
    });
    expect(r.status).toBe(200);
    const b = r.body as { services: Array<{ rss_mb: number | null }>; total_rss_mb: number };
    expect(b.services[0].rss_mb).toBeNull();
    expect(b.total_rss_mb).toBe(0);
  });

  test('total_rss_mb sums rss_mb across services', async () => {
    const lc = [
      LC_HEADER,
      '1\t0\tcom.chorus.a',
      '2\t0\tcom.chorus.b',
      '-\t0\tcom.chorus.c',
    ].join('\n');
    const r = await fetchServices({
      execFile: mockExec({
        launchctl: { stdout: lc },
        ps: { stdout: '1   51200\n2   102400' }, // 50 + 100 = 150
      }),
    });
    const b = r.body as { total_rss_mb: number; running: number };
    expect(b.total_rss_mb).toBe(150);
    expect(b.running).toBe(2);
  });
});
