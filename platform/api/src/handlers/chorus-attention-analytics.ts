/**
 * GET /api/chorus/attention-analytics — Jeff intensity + energy-flow aggregate (extracted #2189).
 *
 * Reads TSV history of 30s keystroke/prompt samples and per-role prompt-time
 * logs, computes headline 24h averages, hour-of-day rhythm (DST-aware),
 * 15-min intensity bands, daily break patterns, typing-vs-prompting windows,
 * energy-flow phase classification (deep_work/directing/dual_load/recovery),
 * transition risks, break effectiveness, role-phase alignment.
 *
 * Dependencies injected — no filesystem access in unit tests.
 */
import * as pathMod from 'path';

export interface AttentionAnalyticsDeps {
  readFile?: (p: string, enc: BufferEncoding) => string;
  exists?: (p: string) => boolean;
  now?: () => number;
  isEDT: (isoDate: string) => boolean;
  tsvPath: string;
  statePath: string;
  promptDir: string;
}

export interface AttentionAnalyticsResult {
  status: number;
  body: Record<string, unknown>;
}

type Row = {
  timestamp: number; intensity: string;
  keys_per_min: number; prompts_1h: number;
  break_count_3h: number; longest_break_min: number;
};

function parseRows(raw: string[]): Row[] {
  const headers = raw[0].split('\t');
  return raw.slice(1).map((line) => {
    const cols = line.split('\t');
    const row: Record<string, unknown> = {};
    headers.forEach((h, i) => {
      row[h] = h === 'intensity' ? cols[i] : parseFloat(cols[i]) || 0;
    });
    return row as Row;
  });
}

function computeHeadline(day: Row[]) {
  const activeDay = day.filter((r) => r.keys_per_min > 0 || r.prompts_1h > 0);
  const avg = (fn: (r: Row) => number) =>
    activeDay.length ? activeDay.reduce((s, r) => s + fn(r), 0) / activeDay.length : 0;
  const intensityCounts: Record<string, number> = { green: 0, yellow: 0, red: 0 };
  day.forEach((r) => { if (intensityCounts[r.intensity] !== undefined) intensityCounts[r.intensity]++; });
  const dayTotal = day.length || 1;
  return {
    avgPromptsHr: Math.round(avg((r) => r.prompts_1h)),
    avgKeysMin: Math.round(avg((r) => r.keys_per_min)),
    avgBreaks3h: Math.round(avg((r) => r.break_count_3h) * 10) / 10,
    greenPct: Math.round((intensityCounts.green / dayTotal) * 100),
    yellowPct: Math.round((intensityCounts.yellow / dayTotal) * 100),
    redPct: Math.round((intensityCounts.red / dayTotal) * 100),
  };
}

function computeHourOfDay(day: Row[], isEDT: (date: string) => boolean) {
  const hourBuckets = { keys: Array(24).fill(0), prompts: Array(24).fill(0), count: Array(24).fill(0) };
  day.forEach((r) => {
    const d = new Date(r.timestamp * 1000);
    const off = isEDT(d.toISOString().slice(0, 10)) ? 4 : 5;
    const hr = (d.getUTCHours() - off + 24) % 24;
    hourBuckets.keys[hr] += r.keys_per_min;
    hourBuckets.prompts[hr] += r.prompts_1h;
    hourBuckets.count[hr]++;
  });
  return Array.from({ length: 24 }, (_, i) => ({
    hour: i,
    avgKeys: hourBuckets.count[i] ? Math.round(hourBuckets.keys[i] / hourBuckets.count[i]) : 0,
    avgPrompts: hourBuckets.count[i] ? Math.round(hourBuckets.prompts[i] / hourBuckets.count[i]) : 0,
  }));
}

function computeIntensityBands(rows: Row[], bandSize: number) {
  const bands: Array<{ timestamp: number; green: number; yellow: number; red: number }> = [];
  if (!rows.length) return bands;
  const counts: Record<string, number> = { green: 0, yellow: 0, red: 0 };
  let bandStart = rows[0].timestamp;
  const flush = () => bands.push({ timestamp: bandStart, green: counts.green, yellow: counts.yellow, red: counts.red });
  for (const r of rows) {
    if (r.timestamp - bandStart >= bandSize) {
      flush();
      bandStart = r.timestamp; counts.green = 0; counts.yellow = 0; counts.red = 0;
    }
    if (counts[r.intensity] !== undefined) counts[r.intensity]++;
  }
  flush();
  return bands;
}

type ReadFileFn = (p: string, enc?: string) => string;
type ExistsFn = (p: string) => boolean;
type ReadRoleTimesFn = (role: string) => number[];

function makeReadRoleTimes(readFile: ReadFileFn, exists: ExistsFn, promptDir: string): ReadRoleTimesFn {
  return (role: string) => {
    const p = pathMod.join(promptDir, `${role}-prompt-times.log`);
    if (!exists(p)) return [];
    return readFile(p, 'utf-8').trim().split('\n')
      .map((t) => parseInt(t, 10)).filter((t) => !isNaN(t));
  };
}

function computeBreakPatterns(rows: Row[]) {
  const dailyBreaks: Record<string, { breaks: number[]; longest: number[] }> = {};
  rows.forEach((r) => {
    const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
    if (!dailyBreaks[d]) dailyBreaks[d] = { breaks: [], longest: [] };
    dailyBreaks[d].breaks.push(r.break_count_3h);
    dailyBreaks[d].longest.push(r.longest_break_min);
  });
  return Object.entries(dailyBreaks).sort().map(([date, data]) => ({
    date,
    avgBreaks: Math.round(Math.max(...data.breaks) * 10) / 10,
    longestBreak: Math.max(...data.longest),
  }));
}

function computeRoleAttention(readRoleTimes: ReadRoleTimesFn, nowSec: number) {
  const out: Record<string, number[]> = { wren: [], silas: [], kade: [] };
  for (const role of ['wren', 'silas', 'kade']) {
    const hourCounts = Array(24).fill(0);
    readRoleTimes(role).forEach((t) => {
      if (nowSec - t < 86400) {
        const hr = new Date((t - 5 * 3600) * 1000).getUTCHours();
        hourCounts[hr]++;
      }
    });
    out[role] = hourCounts;
  }
  return out;
}

function rollWindowAverages<T>(
  day: Row[], windowSize: number,
  emit: (wStart: number, kSum: number, pSum: number, cnt: number) => T,
): T[] {
  const out: T[] = [];
  if (!day.length) return out;
  let wStart = day[0].timestamp;
  let kSum = 0, pSum = 0, cnt = 0;
  for (const r of day) {
    if (r.timestamp - wStart >= windowSize) {
      out.push(emit(wStart, kSum, pSum, cnt));
      wStart = r.timestamp; kSum = 0; pSum = 0; cnt = 0;
    }
    kSum += r.keys_per_min; pSum += r.prompts_1h; cnt++;
  }
  if (cnt > 0) out.push(emit(wStart, kSum, pSum, cnt));
  return out;
}

function computeTypingVsPrompting(day: Row[], windowSize: number) {
  return rollWindowAverages(day, windowSize, (wStart, kSum, pSum, cnt) => ({
    timestamp: wStart,
    keysAvg: cnt ? Math.round(kSum / cnt) : 0,
    promptsAvg: cnt ? Math.round(pSum / cnt) : 0,
  }));
}

function computeDailyStats(rows: Row[]) {
  const summary: Record<string, { active: number; total: number; peakKeys: number; peakPrompts: number; redMin: number }> = {};
  rows.forEach((r) => {
    const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
    if (!summary[d]) summary[d] = { active: 0, total: 0, peakKeys: 0, peakPrompts: 0, redMin: 0 };
    const s = summary[d];
    s.total++;
    if (r.keys_per_min > 0 || r.prompts_1h > 0) s.active++;
    if (r.keys_per_min > s.peakKeys) s.peakKeys = r.keys_per_min;
    if (r.prompts_1h > s.peakPrompts) s.peakPrompts = r.prompts_1h;
    if (r.intensity === 'red') s.redMin += 0.5;
  });
  return Object.entries(summary).sort().map(([date, s]) => ({
    date,
    activeHours: Math.round(((s.active * 0.5) / 60) * 10) / 10,
    peakKeys: Math.round(s.peakKeys),
    peakPrompts: s.peakPrompts,
    redMinutes: Math.round(s.redMin),
  }));
}

function classifyPhase(kAvg: number, pAvg: number): string {
  if (kAvg > 15 && pAvg > 20) return 'dual_load';
  if (kAvg > 15) return 'deep_work';
  if (pAvg > 10) return 'directing';
  return 'recovery';
}

type EnergyFlowEntry = { timestamp: number; phase: string; keys: number; prompts: number; hour: number };

function computeEnergyFlow(day: Row[], windowSize: number): EnergyFlowEntry[] {
  return rollWindowAverages(day, windowSize, (wStart, kSum, pSum, cnt) => {
    const kAvg = cnt ? kSum / cnt : 0;
    const pAvg = cnt ? pSum / cnt : 0;
    const hr = new Date((wStart - 5 * 3600) * 1000).getUTCHours();
    return { timestamp: wStart, phase: classifyPhase(kAvg, pAvg), keys: Math.round(kAvg), prompts: Math.round(pAvg), hour: hr };
  });
}

function computePhaseStats(energyFlow: EnergyFlowEntry[]) {
  const counts: Record<string, number> = { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 };
  energyFlow.forEach((e) => { if (counts[e.phase] !== undefined) counts[e.phase]++; });
  const total = energyFlow.length || 1;
  const phasePcts = {
    deep_work: Math.round((counts.deep_work / total) * 100),
    directing: Math.round((counts.directing / total) * 100),
    dual_load: Math.round((counts.dual_load / total) * 100),
    recovery: Math.round((counts.recovery / total) * 100),
  };
  const transitionRisks: Array<{ timestamp: number; duration: number }> = [];
  let maxDualStreak = 0, curDualStreak = 0, streakStart = 0;
  const closeStreak = () => {
    if (curDualStreak >= 2) transitionRisks.push({ timestamp: streakStart, duration: curDualStreak * 10 });
    if (curDualStreak > maxDualStreak) maxDualStreak = curDualStreak;
    curDualStreak = 0;
  };
  energyFlow.forEach((e) => {
    if (e.phase === 'dual_load') {
      if (curDualStreak === 0) streakStart = e.timestamp;
      curDualStreak++;
      return;
    }
    closeStreak();
  });
  closeStreak();
  return { phasePcts, transitionRisks, maxDualStreak };
}

function computeBreakEffectiveness(day: Row[]) {
  const out: Array<{ breakAt: number; preMeanIntensity: number; postMeanIntensity: number; effective: boolean }> = [];
  for (let i = 1; i < day.length; i++) {
    const gap = day[i].timestamp - day[i - 1].timestamp;
    if (gap <= 600) continue;
    const pre = day.slice(Math.max(0, i - 60), i);
    const post = day.slice(i, Math.min(day.length, i + 60));
    if (pre.length <= 5 || post.length <= 5) continue;
    const score = (rows: Row[]) => rows.reduce((s, r) => s + r.keys_per_min + r.prompts_1h, 0) / rows.length;
    const preScore = score(pre);
    const postScore = score(post);
    out.push({
      breakAt: day[i - 1].timestamp,
      preMeanIntensity: Math.round(preScore),
      postMeanIntensity: Math.round(postScore),
      effective: postScore < preScore * 0.8,
    });
  }
  return out;
}

function computeRolePhaseMap(readRoleTimes: ReadRoleTimesFn, nowSec: number, energyFlow: EnergyFlowEntry[]) {
  const out: Record<string, Record<string, number>> = {
    wren: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
    silas: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
    kade: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
  };
  for (const role of ['wren', 'silas', 'kade']) {
    readRoleTimes(role).filter((t) => nowSec - t < 86400).forEach((t) => {
      for (let i = energyFlow.length - 1; i >= 0; i--) {
        if (t >= energyFlow[i].timestamp) {
          out[role][energyFlow[i].phase]++;
          break;
        }
      }
    });
  }
  return out;
}

export function fetchAttentionAnalytics({
  readFile = () => '',
  exists = () => false,
  now = Date.now,
  isEDT,
  tsvPath,
  statePath,
  promptDir,
}: AttentionAnalyticsDeps): AttentionAnalyticsResult {
  try {
    let current: unknown = null;
    if (exists(statePath)) current = JSON.parse(readFile(statePath, 'utf-8'));

    if (!exists(tsvPath)) {
      return { status: 200, body: { current, history: [], meta: { rows: 0, error: 'No history file' } } };
    }

    const raw = readFile(tsvPath, 'utf-8').trim().split('\n');
    if (raw.length < 2) return { status: 200, body: { current, history: [], meta: { rows: 0 } } };

    const rows = parseRows(raw);
    const nowSec = now() / 1000;
    const day = rows.filter((r) => nowSec - r.timestamp < 86400);

    const headline = computeHeadline(day);
    const hourOfDay = computeHourOfDay(day, isEDT);
    const bands = computeIntensityBands(rows, 900);

    const readRoleTimes = makeReadRoleTimes(readFile, exists, promptDir);
    const breakPatterns = computeBreakPatterns(rows);
    const roleAttention = computeRoleAttention(readRoleTimes, nowSec);
    const typingVsPrompting = computeTypingVsPrompting(day, 1800);
    const dailyStats = computeDailyStats(rows);
    const energyFlow = computeEnergyFlow(day, 600);
    const { phasePcts, transitionRisks, maxDualStreak } = computePhaseStats(energyFlow);
    const breakEffectiveness = computeBreakEffectiveness(day);
    const rolePhaseMap = computeRolePhaseMap(readRoleTimes, nowSec, energyFlow);
    const firstTs = rows[0]?.timestamp || 0;
    const lastTs = rows[rows.length - 1]?.timestamp || 0;

    return {
      status: 200,
      body: {
        headline,
        hourOfDay,
        intensityTimeline: bands,
        breakPatterns,
        roleAttention,
        typingVsPrompting,
        dailyStats,
        energyFlow: {
          flow: energyFlow,
          phasePcts,
          transitionRisks,
          maxDualStreakMin: maxDualStreak * 10,
          breakEffectiveness,
          rolePhaseAlignment: rolePhaseMap,
        },
        current,
        meta: {
          rows: rows.length,
          from: new Date(firstTs * 1000).toISOString(),
          to: new Date(lastTs * 1000).toISOString(),
          daysSpanned: Math.round(((lastTs - firstTs) / 86400) * 10) / 10,
        },
      },
    };
  } catch (err) {
    return { status: 500, body: { error: 'Failed to compute attention analytics', detail: String(err) } };
  }
}
