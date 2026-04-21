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

// eslint-disable-next-line complexity, max-lines-per-function -- #2288 pre-existing threshold violation, tracked for refactor
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

    const headers = raw[0].split('\t');
    const rows = raw.slice(1).map((line) => {
      const cols = line.split('\t');
      const row: Record<string, unknown> = {};
      headers.forEach((h, i) => {
        const v = cols[i];
        row[h] = h === 'intensity' ? v : parseFloat(v) || 0;
      });
      return row as {
        timestamp: number; intensity: string;
        keys_per_min: number; prompts_1h: number;
        break_count_3h: number; longest_break_min: number;
      };
    });

    const nowSec = now() / 1000;
    const day = rows.filter((r) => nowSec - r.timestamp < 86400);
    const activeDay = day.filter((r) => r.keys_per_min > 0 || r.prompts_1h > 0);
    const avgPromptsHr = activeDay.length ? activeDay.reduce((s, r) => s + r.prompts_1h, 0) / activeDay.length : 0;
    const avgKeysMin = activeDay.length ? activeDay.reduce((s, r) => s + r.keys_per_min, 0) / activeDay.length : 0;
    const avgBreaks3h = activeDay.length ? activeDay.reduce((s, r) => s + r.break_count_3h, 0) / activeDay.length : 0;
    const intensityCounts: Record<string, number> = { green: 0, yellow: 0, red: 0 };
    day.forEach((r) => { if (intensityCounts[r.intensity] !== undefined) intensityCounts[r.intensity]++; });
    const dayTotal = day.length || 1;

    const headline = {
      avgPromptsHr: Math.round(avgPromptsHr),
      avgKeysMin: Math.round(avgKeysMin),
      avgBreaks3h: Math.round(avgBreaks3h * 10) / 10,
      greenPct: Math.round((intensityCounts.green / dayTotal) * 100),
      yellowPct: Math.round((intensityCounts.yellow / dayTotal) * 100),
      redPct: Math.round((intensityCounts.red / dayTotal) * 100),
    };

    const hourBuckets = { keys: Array(24).fill(0), prompts: Array(24).fill(0), count: Array(24).fill(0) };
    day.forEach((r) => {
      const d = new Date(r.timestamp * 1000);
      const off = isEDT(d.toISOString().slice(0, 10)) ? 4 : 5;
      const hr = (d.getUTCHours() - off + 24) % 24;
      hourBuckets.keys[hr] += r.keys_per_min;
      hourBuckets.prompts[hr] += r.prompts_1h;
      hourBuckets.count[hr]++;
    });
    const hourOfDay = Array.from({ length: 24 }, (_, i) => ({
      hour: i,
      avgKeys: hourBuckets.count[i] ? Math.round(hourBuckets.keys[i] / hourBuckets.count[i]) : 0,
      avgPrompts: hourBuckets.count[i] ? Math.round(hourBuckets.prompts[i] / hourBuckets.count[i]) : 0,
    }));

    const bandSize = 900;
    const bands: Array<{ timestamp: number; green: number; yellow: number; red: number }> = [];
    if (rows.length) {
      let bandStart = rows[0].timestamp;
      let g = 0, y = 0, r2 = 0;
      for (const r of rows) {
        if (r.timestamp - bandStart >= bandSize) {
          bands.push({ timestamp: bandStart, green: g, yellow: y, red: r2 });
          bandStart = r.timestamp; g = 0; y = 0; r2 = 0;
        }
        if (r.intensity === 'green') g++;
        else if (r.intensity === 'yellow') y++;
        else if (r.intensity === 'red') r2++;
      }
      bands.push({ timestamp: bandStart, green: g, yellow: y, red: r2 });
    }

    const dailyBreaks: Record<string, { breaks: number[]; longest: number[] }> = {};
    rows.forEach((r) => {
      const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
      if (!dailyBreaks[d]) dailyBreaks[d] = { breaks: [], longest: [] };
      dailyBreaks[d].breaks.push(r.break_count_3h);
      dailyBreaks[d].longest.push(r.longest_break_min);
    });
    const breakPatterns = Object.entries(dailyBreaks).sort().map(([date, data]) => ({
      date,
      avgBreaks: Math.round(Math.max(...data.breaks) * 10) / 10,
      longestBreak: Math.max(...data.longest),
    }));

    const roleAttention: Record<string, number[]> = { wren: [], silas: [], kade: [] };
    const readRoleTimes = (role: string): number[] => {
      const p = pathMod.join(promptDir, `${role}-prompt-times.log`);
      if (!exists(p)) return [];
      return readFile(p, 'utf-8').trim().split('\n')
        .map((t) => parseInt(t, 10)).filter((t) => !isNaN(t));
    };
    for (const role of ['wren', 'silas', 'kade']) {
      const times = readRoleTimes(role);
      const hourCounts = Array(24).fill(0);
      times.forEach((t) => {
        if (nowSec - t < 86400) {
          const hr = new Date((t - 5 * 3600) * 1000).getUTCHours();
          hourCounts[hr]++;
        }
      });
      roleAttention[role] = hourCounts;
    }

    const windowSize = 1800;
    const typingVsPrompting: Array<{ timestamp: number; keysAvg: number; promptsAvg: number }> = [];
    if (day.length) {
      let wStart = day[0].timestamp;
      let kSum = 0, pSum = 0, cnt = 0;
      for (const r of day) {
        if (r.timestamp - wStart >= windowSize) {
          typingVsPrompting.push({
            timestamp: wStart,
            keysAvg: cnt ? Math.round(kSum / cnt) : 0,
            promptsAvg: cnt ? Math.round(pSum / cnt) : 0,
          });
          wStart = r.timestamp; kSum = 0; pSum = 0; cnt = 0;
        }
        kSum += r.keys_per_min; pSum += r.prompts_1h; cnt++;
      }
      if (cnt > 0) typingVsPrompting.push({
        timestamp: wStart,
        keysAvg: Math.round(kSum / cnt),
        promptsAvg: Math.round(pSum / cnt),
      });
    }

    const dailySummary: Record<string, { active: number; total: number; peakKeys: number; peakPrompts: number; redMin: number }> = {};
    rows.forEach((r) => {
      const d = new Date((r.timestamp - 5 * 3600) * 1000).toISOString().slice(0, 10);
      if (!dailySummary[d]) dailySummary[d] = { active: 0, total: 0, peakKeys: 0, peakPrompts: 0, redMin: 0 };
      dailySummary[d].total++;
      if (r.keys_per_min > 0 || r.prompts_1h > 0) dailySummary[d].active++;
      if (r.keys_per_min > dailySummary[d].peakKeys) dailySummary[d].peakKeys = r.keys_per_min;
      if (r.prompts_1h > dailySummary[d].peakPrompts) dailySummary[d].peakPrompts = r.prompts_1h;
      if (r.intensity === 'red') dailySummary[d].redMin += 0.5;
    });
    const dailyStats = Object.entries(dailySummary).sort().map(([date, s]) => ({
      date,
      activeHours: Math.round(((s.active * 0.5) / 60) * 10) / 10,
      peakKeys: Math.round(s.peakKeys),
      peakPrompts: s.peakPrompts,
      redMinutes: Math.round(s.redMin),
    }));

    const firstTs = rows[0]?.timestamp || 0;
    const lastTs = rows[rows.length - 1]?.timestamp || 0;

    const WINDOW = 600;
    const classify = (kAvg: number, pAvg: number): string =>
      kAvg > 15 && pAvg > 20 ? 'dual_load' :
      kAvg > 15 ? 'deep_work' :
      pAvg > 10 ? 'directing' : 'recovery';

    const energyFlow: Array<{ timestamp: number; phase: string; keys: number; prompts: number; hour: number }> = [];
    if (day.length) {
      let wStart = day[0].timestamp;
      let kSum = 0, pSum = 0, cnt = 0;
      const flush = () => {
        const kAvg = cnt ? kSum / cnt : 0;
        const pAvg = cnt ? pSum / cnt : 0;
        const hr = new Date((wStart - 5 * 3600) * 1000).getUTCHours();
        energyFlow.push({ timestamp: wStart, phase: classify(kAvg, pAvg), keys: Math.round(kAvg), prompts: Math.round(pAvg), hour: hr });
      };
      for (const r of day) {
        if (r.timestamp - wStart >= WINDOW) {
          flush();
          wStart = r.timestamp; kSum = 0; pSum = 0; cnt = 0;
        }
        kSum += r.keys_per_min; pSum += r.prompts_1h; cnt++;
      }
      if (cnt > 0) flush();
    }

    const phaseCounts: Record<string, number> = { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 };
    energyFlow.forEach((e) => { if (phaseCounts[e.phase] !== undefined) phaseCounts[e.phase]++; });
    const phaseTotal = energyFlow.length || 1;
    const phasePcts = {
      deep_work: Math.round((phaseCounts.deep_work / phaseTotal) * 100),
      directing: Math.round((phaseCounts.directing / phaseTotal) * 100),
      dual_load: Math.round((phaseCounts.dual_load / phaseTotal) * 100),
      recovery: Math.round((phaseCounts.recovery / phaseTotal) * 100),
    };

    let maxDualStreak = 0, curDualStreak = 0, streakStart = 0;
    const transitionRisks: Array<{ timestamp: number; duration: number }> = [];
    energyFlow.forEach((e) => {
      if (e.phase === 'dual_load') {
        if (curDualStreak === 0) streakStart = e.timestamp;
        curDualStreak++;
      } else {
        if (curDualStreak >= 2) transitionRisks.push({ timestamp: streakStart, duration: curDualStreak * 10 });
        if (curDualStreak > maxDualStreak) maxDualStreak = curDualStreak;
        curDualStreak = 0;
      }
    });
    if (curDualStreak >= 2) transitionRisks.push({ timestamp: streakStart, duration: curDualStreak * 10 });
    if (curDualStreak > maxDualStreak) maxDualStreak = curDualStreak;

    const breakEffectiveness: Array<{ breakAt: number; preMeanIntensity: number; postMeanIntensity: number; effective: boolean }> = [];
    for (let i = 1; i < day.length; i++) {
      const gap = day[i].timestamp - day[i - 1].timestamp;
      if (gap > 600) {
        const pre = day.slice(Math.max(0, i - 60), i);
        const post = day.slice(i, Math.min(day.length, i + 60));
        if (pre.length > 5 && post.length > 5) {
          const preScore = pre.reduce((s, r) => s + r.keys_per_min + r.prompts_1h, 0) / pre.length;
          const postScore = post.reduce((s, r) => s + r.keys_per_min + r.prompts_1h, 0) / post.length;
          breakEffectiveness.push({
            breakAt: day[i - 1].timestamp,
            preMeanIntensity: Math.round(preScore),
            postMeanIntensity: Math.round(postScore),
            effective: postScore < preScore * 0.8,
          });
        }
      }
    }

    const rolePhaseMap: Record<string, Record<string, number>> = {
      wren: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
      silas: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
      kade: { deep_work: 0, directing: 0, dual_load: 0, recovery: 0 },
    };
    for (const role of ['wren', 'silas', 'kade']) {
      const times = readRoleTimes(role).filter((t) => nowSec - t < 86400);
      times.forEach((t) => {
        for (let i = energyFlow.length - 1; i >= 0; i--) {
          if (t >= energyFlow[i].timestamp) {
            rolePhaseMap[role][energyFlow[i].phase]++;
            break;
          }
        }
      });
    }

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
