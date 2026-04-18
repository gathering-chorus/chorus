/**
 * chorus-disk handler — unit tests (#2189).
 *
 * GET /api/chorus/disk reports disk usage. Two shell calls:
 *   - diskutil info / → container total + free bytes
 *   - osascript (Finder) → free bytes including purgeable
 * Finder free is preferred; falls back to container free when osascript fails.
 *
 * Tests describe Jeff-visible response shape:
 *   - diskutil failure → 500 with { error, detail }
 *   - both succeed → Finder free wins, usedPct computed
 *   - Finder fails, diskutil ok → container free wins
 *   - usedPct >= 90 → warning=true; >= 95 → critical=true
 *   - usedPct < 90 → both false
 *   - machine field always 'Library'
 */
import { fetchDisk, type ExecFileFn } from '../../src/handlers/chorus-disk';

const DISKUTIL_OUT = `
   Container Total Space:     500 GB (500000000000 Bytes) (exactly 1000000000 512-Byte-Units)
   Container Free Space:      100 GB (100000000000 Bytes) (exactly 200000000 512-Byte-Units)
`;

function mockExec(outputs: Record<string, { stdout?: string; reject?: Error }>): ExecFileFn {
  return async (cmd: string, _args: string[], _opts?: { timeout?: number }) => {
    const spec = outputs[cmd];
    if (!spec) throw new Error(`unexpected exec: ${cmd}`);
    if (spec.reject) throw spec.reject;
    return { stdout: spec.stdout ?? '', stderr: '' };
  };
}

describe('fetchDisk (#2189 /api/chorus/disk)', () => {
  test('diskutil failure returns 500 with error envelope', async () => {
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { reject: new Error('not found') },
      }),
    });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'diskutil info failed', detail: 'not found' });
  });

  test('both calls succeed: Finder free wins, usedPct computed from it', async () => {
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { stdout: DISKUTIL_OUT },
        '/usr/bin/osascript': { stdout: '150000000000' },
      }),
    });
    expect(r.status).toBe(200);
    const b = r.body as Record<string, unknown>;
    expect(b.machine).toBe('Library');
    expect(b.total_bytes).toBe(500_000_000_000);
    expect(b.container_free_bytes).toBe(100_000_000_000);
    expect(b.finder_free_bytes).toBe(150_000_000_000);
    expect(b.free_bytes).toBe(150_000_000_000);
    expect(b.used_bytes).toBe(350_000_000_000);
    expect(b.used_pct).toBe(70);
    expect(b.warning).toBe(false);
    expect(b.critical).toBe(false);
  });

  test('Finder fails, diskutil ok → container free wins', async () => {
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { stdout: DISKUTIL_OUT },
        '/usr/bin/osascript': { reject: new Error('Finder unavailable') },
      }),
    });
    expect(r.status).toBe(200);
    const b = r.body as Record<string, unknown>;
    expect(b.finder_free_bytes).toBeNull();
    expect(b.free_bytes).toBe(100_000_000_000);
    expect(b.used_pct).toBe(80);
  });

  test('usedPct 90 → warning true, critical false', async () => {
    const diskOut = `
   Container Total Space:     100 GB (100000000000 Bytes)
   Container Free Space:       10 GB (10000000000 Bytes)
`;
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { stdout: diskOut },
        '/usr/bin/osascript': { stdout: '10000000000' },
      }),
    });
    const b = r.body as Record<string, unknown>;
    expect(b.used_pct).toBe(90);
    expect(b.warning).toBe(true);
    expect(b.critical).toBe(false);
  });

  test('usedPct 95 → warning AND critical true', async () => {
    const diskOut = `
   Container Total Space:     100 GB (100000000000 Bytes)
   Container Free Space:        5 GB (5000000000 Bytes)
`;
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { stdout: diskOut },
        '/usr/bin/osascript': { stdout: '5000000000' },
      }),
    });
    const b = r.body as Record<string, unknown>;
    expect(b.used_pct).toBe(95);
    expect(b.warning).toBe(true);
    expect(b.critical).toBe(true);
  });

  test('diskutil output missing labels → null size fields', async () => {
    const r = await fetchDisk({
      execFile: mockExec({
        '/usr/sbin/diskutil': { stdout: 'unparseable output with no labels' },
        '/usr/bin/osascript': { reject: new Error('na') },
      }),
    });
    expect(r.status).toBe(200);
    const b = r.body as Record<string, unknown>;
    expect(b.total).toBeNull();
    expect(b.free).toBeNull();
    expect(b.total_bytes).toBeNull();
    expect(b.used_pct).toBeNull();
    expect(b.warning).toBe(false);
    expect(b.critical).toBe(false);
  });
});
