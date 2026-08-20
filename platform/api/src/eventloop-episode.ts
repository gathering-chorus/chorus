// #3742 — one freeze, one incident.
//
// Two detectors watch the same event loop by design (#3082): an external probe
// that cannot be blocked by the loop it measures, and the in-process `blocked`
// library that rides it. That redundancy is correct — and it means a single
// freeze produces at least two events.
//
// Measured 2026-08-04: every big block was preceded ~15s by a probe timeout
// reading EXACTLY 8000ms. One freeze, two clocks. The probe hits its ceiling
// mid-freeze and reports its cap; the in-process detector fires when the loop
// unfreezes and reports the true length. Both became separate alarms, so a
// ~20s stall arrived as a stream of alerts rather than one incident.
//
// This module is pure: readings in, episodes out. No timers, no I/O — so the
// grouping can be proven against the real measured pairs without a busy box.

import * as fs from 'fs';

/** One detector's reading of a block. */
export interface BlockReading {
  /** ISO timestamp — storage contract, rendered to Boston at the edge (#3093). */
  ts: string;
  duration_ms: number;
  /** Which vantage saw it. Both are kept; neither is authoritative alone. */
  source: 'probe' | 'in-process';
  op: string;
  stack?: string[];
}

export interface Episode {
  key: string;
  startedAt: string;
  readings: BlockReading[];
}

export interface EpisodeSummary {
  key: string;
  startedAt: string;
  /** How many readings the episode absorbed — the alerts Jeff no longer gets. */
  count: number;
  /** Longest UNCAPPED reading. A capped probe reading is a floor, not a duration. */
  worst_ms: number;
  /** True when every reading was capped: the freeze is real, its length unknown. */
  duration_unknown: boolean;
  sources: Array<'probe' | 'in-process'>;
  ops: string[];
}

/** The probe's timeout ceiling. A probe reading at exactly the ceiling means
 *  "frozen for at least this long", never "frozen for this long" — the card's
 *  own note: treat 8000 as `freeze >= 8s, duration unknown`. */
export const PROBE_CEILING_MS = 8000;

/** Default window. The measured pair-pattern had ~15s between the probe's cap
 *  and the in-process completion, so the window must comfortably exceed that
 *  while staying well short of the ~6-7min storm cadence. */
export const DEFAULT_EPISODE_WINDOW_MS = 30_000;

/** Is this reading the probe reporting its own ceiling rather than a duration? */
export function isCappedReading(r: BlockReading): boolean {
  return r.source === 'probe' && r.duration_ms >= PROBE_CEILING_MS;
}

/** Stable key for an episode, derived from when it started. */
export function episodeKey(startedAt: string): string {
  return `ep-${Date.parse(startedAt)}`;
}

/** Group readings into episodes: consecutive readings within `windowMs` of the
 *  previous one belong to the same freeze. Input order does not matter. */
export function groupIntoEpisodes(
  readings: BlockReading[],
  windowMs: number = DEFAULT_EPISODE_WINDOW_MS,
): Episode[] {
  if (readings.length === 0) return [];
  const sorted = [...readings].sort((a, b) => Date.parse(a.ts) - Date.parse(b.ts));
  const episodes: Episode[] = [];
  let current: Episode | null = null;
  let lastTs = 0;

  for (const r of sorted) {
    const t = Date.parse(r.ts);
    if (current === null || t - lastTs > windowMs) {
      current = { key: episodeKey(r.ts), startedAt: r.ts, readings: [r] };
      episodes.push(current);
    } else {
      current.readings.push(r);
    }
    lastTs = t;
  }
  return episodes;
}

/** Collapse an episode to the one thing worth alerting on. */
export function summarizeEpisode(ep: Episode): EpisodeSummary {
  const uncapped = ep.readings.filter((r) => !isCappedReading(r));
  const duration_unknown = uncapped.length === 0;
  // With no uncapped reading we still know the freeze lasted at least the
  // largest capped value — a floor, flagged as such rather than dressed up.
  const pool = duration_unknown ? ep.readings : uncapped;
  const worst_ms = Math.max(...pool.map((r) => r.duration_ms));

  const sources = Array.from(new Set(ep.readings.map((r) => r.source)));
  const ops = Array.from(new Set(ep.readings.map((r) => r.op)));

  return {
    key: ep.key,
    startedAt: ep.startedAt,
    count: ep.readings.length,
    worst_ms,
    duration_unknown,
    sources,
    ops,
  };
}

/** One line for a human. Says how many readings it absorbed, so a collapsed
 *  storm never looks like a single small hiccup. */
export function formatEpisode(s: EpisodeSummary, display: string): string {
  const dur = s.duration_unknown
    ? `at least ${s.worst_ms}ms (every reading hit a detector ceiling — true length unknown)`
    : `${s.worst_ms}ms`;
  const absorbed = s.count > 1 ? ` ${s.count} readings from ${s.sources.join(' + ')}.` : '';
  const ops = s.ops.filter((o) => o !== 'unknown');
  const opNote = ops.length > 0 ? ` Ops seen: ${ops.join(', ')}.` : '';
  return (
    `chorus-api event loop blocked ${dur} at ${display}.` +
    absorbed +
    opNote +
    ' One episode; no cause inferred.'
  );
}

/** Shared announce-gate across BOTH detectors.
 *
 *  Each detector already throttles its own nudges, but independently — so one
 *  freeze still delivered two alarms, one per vantage, and neither throttle
 *  could see the other. This gate is the shared state: the first reading of an
 *  episode announces, every later reading of the SAME episode is absorbed.
 *
 *  It is deliberately not a mute button. A reading outside the window opens a
 *  new episode and announces again — proven by the negative test, because a
 *  gate that silences real incidents is worse than the flood it replaced. */
export class EpisodeGate {
  private current: Episode | null = null;
  private lastTs = 0;

  constructor(private readonly windowMs: number = DEFAULT_EPISODE_WINDOW_MS) {}

  /** Record the reading; true iff it OPENS a new episode (i.e. announce it). */
  shouldAnnounce(r: BlockReading): boolean {
    const t = Date.parse(r.ts);
    if (this.current === null || t - this.lastTs > this.windowMs) {
      this.current = { key: episodeKey(r.ts), startedAt: r.ts, readings: [r] };
      this.lastTs = t;
      return true;
    }
    this.current.readings.push(r);
    this.lastTs = t;
    return false;
  }

  /** The episode in progress, with everything it has absorbed — so a collapsed
   *  storm can still be reported as a storm rather than a single hiccup. */
  currentSummary(): EpisodeSummary | null {
    return this.current === null ? null : summarizeEpisode(this.current);
  }

  currentKey(): string | null {
    return this.current?.key ?? null;
  }
}

/** Cross-process shared gate.
 *
 *  The two detectors are separate PROCESSES (#3082): the in-process one lives
 *  inside chorus-api, the probe runs as its own LaunchAgent worker. In-memory
 *  state cannot span them, so the episode claim lives in a small JSON file both
 *  read and write.
 *
 *  Failure policy is ANNOUNCE. If the shared state is unreadable or corrupt,
 *  every reading opens a new episode and Jeff still hears about the freeze. A
 *  monitor that goes quiet because its own bookkeeping broke is the worst
 *  failure mode available to it — losing an alarm costs more than repeating one.
 */
export class FileEpisodeGate {
  constructor(
    private readonly statePath: string,
    private readonly windowMs: number = DEFAULT_EPISODE_WINDOW_MS,
  ) {}

  private read(): { key: string; lastTs: number } | null {
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- the path is operator-configured (env or a constant default), never user input; both detectors must open the SAME file for the episode claim to work (#3742)
      const raw = fs.readFileSync(this.statePath, 'utf8');
      const parsed = JSON.parse(raw) as { key?: string; lastTs?: number };
      if (typeof parsed.key !== 'string' || typeof parsed.lastTs !== 'number') return null;
      return { key: parsed.key, lastTs: parsed.lastTs };
    } catch {
      return null;
    }
  }

  private write(key: string, lastTs: number): void {
    try {
      // Write-then-rename so a reader never sees a half-written claim.
      const tmp = `${this.statePath}.tmp${process.pid}`;
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- the path is operator-configured (env or a constant default), never user input; both detectors must open the SAME file for the episode claim to work (#3742)
      fs.writeFileSync(tmp, JSON.stringify({ key, lastTs }));
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- the path is operator-configured (env or a constant default), never user input; both detectors must open the SAME file for the episode claim to work (#3742)
      fs.renameSync(tmp, this.statePath);
    } catch {
      // Cannot persist the claim: the next reading will announce. Loud, not silent.
    }
  }

  /** True iff this reading OPENS a new episode — i.e. announce it. */
  shouldAnnounce(r: BlockReading): boolean {
    const t = Date.parse(r.ts);
    const prior = this.read();
    if (prior === null || t - prior.lastTs > this.windowMs) {
      this.write(episodeKey(r.ts), t);
      return true;
    }
    this.write(prior.key, t);
    return false;
  }

  currentKey(): string | null {
    return this.read()?.key ?? null;
  }
}
