/**
 * chorus-attention-analytics handler — unit tests (#2189).
 *
 * Synthetic TSV + mock fs, deterministic now(). Tests verify contract
 * boundaries: file-missing paths, empty/short TSV, headline math, phase
 * classification, transition risk (≥2 consecutive dual_load), rolePhase
 * lookup, error path.
 */
import {
  fetchAttentionAnalytics,
  type AttentionAnalyticsDeps,
} from '../../src/handlers/chorus-attention-analytics';

const NOW_SEC = 1_700_000_000;
const NOW_MS = NOW_SEC * 1000;
const isEDT = () => true;

function deps(files: Record<string, string>, overrides: Partial<AttentionAnalyticsDeps> = {}): AttentionAnalyticsDeps {
  return {
    readFile: (p) => files[p] ?? '',
    exists: (p) => Object.prototype.hasOwnProperty.call(files, p),
    now: () => NOW_MS,
    isEDT,
    tsvPath: '/fake/history.tsv',
    statePath: '/fake/state.json',
    promptDir: '/fake/prompts',
    ...overrides,
  };
}

function tsv(rows: Array<{
  timestamp: number;
  intensity: string;
  keys_per_min?: number;
  prompts_1h?: number;
  break_count_3h?: number;
  longest_break_min?: number;
}>): string {
  const header = 'timestamp\tkeys_per_min\tprompts_1h\tbreak_count_3h\tlongest_break_min\tintensity';
  const lines = rows.map((r) =>
    [r.timestamp, r.keys_per_min ?? 0, r.prompts_1h ?? 0, r.break_count_3h ?? 0, r.longest_break_min ?? 0, r.intensity].join('\t'),
  );
  return [header, ...lines].join('\n');
}

describe('fetchAttentionAnalytics (#2189 /api/chorus/attention-analytics)', () => {
  test('no TSV file → 200 with empty history + meta error', () => {
    const r = fetchAttentionAnalytics(deps({}));
    expect(r.status).toBe(200);
    expect(r.body).toMatchObject({
      current: null,
      history: [],
      meta: { rows: 0, error: 'No history file' },
    });
  });

  test('state file present → current populated from JSON', () => {
    const r = fetchAttentionAnalytics(
      deps({
        '/fake/state.json': JSON.stringify({ intensity: 'green', note: 'ok' }),
      }),
    );
    expect(r.status).toBe(200);
    expect((r.body as { current: unknown }).current).toEqual({ intensity: 'green', note: 'ok' });
  });

  test('header-only TSV → empty history, rows=0', () => {
    const r = fetchAttentionAnalytics(deps({
      '/fake/history.tsv': 'timestamp\tkeys_per_min\tintensity',
    }));
    const b = r.body as { meta: { rows: number }; history: unknown[] };
    expect(b.meta.rows).toBe(0);
    expect(b.history).toEqual([]);
  });

  test('headline averages computed over ACTIVE 24h rows', () => {
    const rows = [
      { timestamp: NOW_SEC - 1000, intensity: 'green', keys_per_min: 20, prompts_1h: 10, break_count_3h: 2, longest_break_min: 15 },
      { timestamp: NOW_SEC - 2000, intensity: 'green', keys_per_min: 40, prompts_1h: 30, break_count_3h: 4, longest_break_min: 20 },
      { timestamp: NOW_SEC - 3000, intensity: 'green', keys_per_min: 0, prompts_1h: 0, break_count_3h: 0, longest_break_min: 0 }, // inactive, excluded
    ];
    const r = fetchAttentionAnalytics(deps({ '/fake/history.tsv': tsv(rows) }));
    const b = r.body as { headline: { avgKeysMin: number; avgPromptsHr: number; greenPct: number } };
    expect(b.headline.avgKeysMin).toBe(30); // (20+40)/2
    expect(b.headline.avgPromptsHr).toBe(20); // (10+30)/2
    expect(b.headline.greenPct).toBe(100); // all 3 day rows are green
  });

  test('energy-flow phase classification: dual_load needs both keys>15 AND prompts>20', () => {
    const rows: Array<{ timestamp: number; intensity: string; keys_per_min: number; prompts_1h: number }> = [];
    // 10-min window (WINDOW=600s) of dual_load samples
    for (let i = 0; i < 30; i++) {
      rows.push({
        timestamp: NOW_SEC - 1000 + i * 20,
        intensity: 'red',
        keys_per_min: 30,
        prompts_1h: 25,
      });
    }
    const r = fetchAttentionAnalytics(deps({ '/fake/history.tsv': tsv(rows) }));
    const b = r.body as { energyFlow: { flow: Array<{ phase: string }>; phasePcts: Record<string, number> } };
    expect(b.energyFlow.flow.some((e) => e.phase === 'dual_load')).toBe(true);
    expect(b.energyFlow.phasePcts.dual_load).toBeGreaterThan(0);
  });

  test('phase=deep_work when keys>15 but prompts low', () => {
    const rows: Array<{ timestamp: number; intensity: string; keys_per_min: number; prompts_1h: number }> = [];
    for (let i = 0; i < 30; i++) {
      rows.push({ timestamp: NOW_SEC - 1000 + i * 20, intensity: 'green', keys_per_min: 30, prompts_1h: 2 });
    }
    const r = fetchAttentionAnalytics(deps({ '/fake/history.tsv': tsv(rows) }));
    const b = r.body as { energyFlow: { flow: Array<{ phase: string }> } };
    expect(b.energyFlow.flow.every((e) => e.phase === 'deep_work')).toBe(true);
  });

  test('phase=recovery when both low', () => {
    const rows: Array<{ timestamp: number; intensity: string; keys_per_min: number; prompts_1h: number }> = [];
    for (let i = 0; i < 30; i++) {
      rows.push({ timestamp: NOW_SEC - 1000 + i * 20, intensity: 'green', keys_per_min: 1, prompts_1h: 1 });
    }
    const r = fetchAttentionAnalytics(deps({ '/fake/history.tsv': tsv(rows) }));
    const b = r.body as { energyFlow: { flow: Array<{ phase: string }>; phasePcts: Record<string, number> } };
    expect(b.energyFlow.phasePcts.recovery).toBe(100);
  });

  test('transitionRisks only fires for ≥2 consecutive dual_load windows', () => {
    // Build day data spanning multiple 10-min windows with varying loads
    const rows: Array<{ timestamp: number; intensity: string; keys_per_min: number; prompts_1h: number }> = [];
    // Two consecutive 10-min windows of dual_load, then recovery
    for (let w = 0; w < 2; w++) {
      for (let i = 0; i < 30; i++) {
        rows.push({ timestamp: NOW_SEC - 3000 + w * 600 + i * 20, intensity: 'red', keys_per_min: 30, prompts_1h: 25 });
      }
    }
    for (let i = 0; i < 30; i++) {
      rows.push({ timestamp: NOW_SEC - 1800 + i * 20, intensity: 'green', keys_per_min: 1, prompts_1h: 1 });
    }
    const r = fetchAttentionAnalytics(deps({ '/fake/history.tsv': tsv(rows) }));
    const b = r.body as { energyFlow: { transitionRisks: Array<{ duration: number }>; maxDualStreakMin: number } };
    expect(b.energyFlow.transitionRisks.length).toBeGreaterThan(0);
    expect(b.energyFlow.maxDualStreakMin).toBeGreaterThanOrEqual(20);
  });

  test('roleAttention bucketed into 24h when log present', () => {
    const rows = [{ timestamp: NOW_SEC - 500, intensity: 'green', keys_per_min: 5, prompts_1h: 5 }];
    const now24hAgo = NOW_SEC - 500; // Boston-adjusted hour = this timestamp
    const r = fetchAttentionAnalytics(deps({
      '/fake/history.tsv': tsv(rows),
      '/fake/prompts/kade-prompt-times.log': `${now24hAgo}\n${now24hAgo}\n${now24hAgo - 86500}`, // 3rd is >24h, excluded
    }));
    const b = r.body as { roleAttention: Record<string, number[]> };
    const total = b.roleAttention.kade.reduce((s, x) => s + x, 0);
    expect(total).toBe(2);
  });

  test('malformed JSON in state.json → 500 error envelope', () => {
    const r = fetchAttentionAnalytics(deps({ '/fake/state.json': '{not json' }));
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toBe('Failed to compute attention analytics');
  });
});
