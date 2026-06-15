/**
 * flow-report — the card cycle/step/error fitness function, pure core (#3269).
 *
 * Spine events in → structured flow JSON out: per-card cycle time, per-step
 * times, errors/warnings enumerated as children, error classes ranked across
 * cards. This is the standing form of the 06-06 one-off report
 * (~/.chorus/reports/card-cycle-report.html) and the measure behind #3266's
 * walk-away bar. Pure (no IO) — the CLI wrapper feeds it Loki events; the
 * chorus_flow_report MCP tool execs the CLI (never computes on a serving loop).
 */

export interface FlowEvent {
  ts: number; // epoch ms
  event: string;
  card_id?: number;
  role?: string;
  detail?: string;
}

export interface CardFlow {
  card: number;
  role: string;
  landed: boolean;
  /** epoch ms of the card's most recent event — drives the page's time filter. */
  lastEventTs: number;
  cycleS: number;
  steps: {
    workS: number | null;
    pushS: number | null;
    buildS: number | null;
    deployS: number | null;
    demoS: number | null;
    mergeS: number | null;
    finalS: number | null;
  };
  errors: Array<{ ts: number; event: string; detail: string }>;
}

export interface FlowReport {
  cards: CardFlow[];
  errorClasses: Array<{ event: string; count: number }>;
  totals: { cards: number; landed: number; withErrors: number; errorEvents: number };
  /** Overall cycle stats across LANDED cards (seconds) — the leading indicator
   *  (Jeff, 2026-02-21: "instrument cycle time and lead time"). Lead time (card
   *  created→accepted) needs the board join — #3266's layer, not computed here. */
  cycleStats: { landedCards: number; medianS: number | null; avgS: number | null; p90S: number | null };
}

function percentile(sorted: number[], p: number): number | null {
  if (sorted.length === 0) return null;
  const idx = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[Math.max(0, idx)];
}

/** Lifecycle checkpoints in order. A step's duration = its checkpoint minus the
 *  previous OBSERVED checkpoint (retries supersede: last occurrence wins). */
const CHECKPOINTS: Array<{ step: keyof CardFlow['steps']; events: string[] }> = [
  { step: 'workS', events: ['commit.completed'] },
  { step: 'pushS', events: ['push.completed'] },
  { step: 'buildS', events: ['build.completed'] },
  { step: 'deployS', events: ['deploy.completed'] },
  { step: 'demoS', events: ['werk.presented', 'demo.presented'] },  // #3410: demo.verdict synthesis retired; demoS tracks the real demo.presented
  { step: 'mergeS', events: ['werk.landed', 'merge.completed'] },
  { step: 'finalS', events: ['card.accepted', 'finalize.completed'] },
];

const START_EVENTS = ['pull.completed', 'card.pulled', 'pull.started'];
const LANDED_EVENTS = ['card.accepted', 'werk.landed', 'finalize.completed'];

/** An error/warning event by name shape: *.failed / *.refused / *.error /
 *  *.rolledback / *.diverged — the classes the 06-06 report enumerated. */
export function isErrorEvent(event: string): boolean {
  return /\.(failed|refused|error|rolledback|diverged)$/.test(event) || event === 'mcp.tool.error';
}

/** #3397 — plausibility band for an event's epoch-ms timestamp. Some spine
 *  emitters have shipped corrupt clocks: BSD `date %3N` fails on macOS and
 *  yields ~1970-magnitude values (normalizeLine strips the trailing N at #3266
 *  but the magnitude stays ~100x too small), and cross-source skew can stamp
 *  the future. A ts outside this band cannot be trusted to bound a duration, so
 *  it must never define first/last or a step checkpoint — otherwise ONE bad
 *  event poisons cycleS/mergeS into the billions of seconds (the "55-year
 *  median" the page showed). Absolute (not now-relative) so the core stays pure
 *  and the tests stay hermetic; the band is decade-wide, far wider than any
 *  report window, and only needs a bump past 2035. Timing-only: poison events
 *  are still counted as errors and still flip `landed`. */
const TS_PLAUSIBLE_MIN = Date.UTC(2024, 0, 1); // 2024-01-01
const TS_PLAUSIBLE_MAX = Date.UTC(2035, 0, 1); // 2035-01-01

function plausibleTs(ts: number): boolean {
  return Number.isFinite(ts) && ts >= TS_PLAUSIBLE_MIN && ts <= TS_PLAUSIBLE_MAX;
}

// eslint-disable-next-line complexity, sonarjs/cognitive-complexity, max-lines-per-function -- cohesive single-pass per-card flow aggregation (step timings + verdicts in one walk); genuine decomposition is its own card, not a metric-chase (#3429)
export function aggregateFlow(events: FlowEvent[]): FlowReport {
  const byCard = new Map<number, FlowEvent[]>();
  for (const e of events) {
    if (typeof e.card_id !== 'number') continue;
    let list = byCard.get(e.card_id);
    if (!list) {
      list = [];
      byCard.set(e.card_id, list);
    }
    list.push(e);
  }

  const cards: CardFlow[] = [];
  const classCounts = new Map<string, number>();
  let landedCount = 0;
  let withErrors = 0;
  let errorEvents = 0;

  for (const [card, list] of byCard) {
    list.sort((a, b) => a.ts - b.ts);

    // #3397 — cycle boundaries and checkpoints come ONLY from plausibly-clocked
    // events, so a corrupt 1970/2081 timestamp can't define first/last (cycleS)
    // or a step boundary (mergeS). Fall back to the raw bounds only if a card
    // has no plausible event at all (degenerate — keeps cycleS finite, not NaN).
    const timed = list.filter((e) => plausibleTs(e.ts));
    const first = (timed[0] ?? list[0]).ts;
    const last = (timed[timed.length - 1] ?? list[list.length - 1]).ts;
    const cycleS = timed.length ? Math.round((last - first) / 1000) : 0;

    // Last occurrence per checkpoint (retries supersede). Poison-clocked events
    // are skipped here so they neither set nor advance a checkpoint boundary.
    const checkpointTs = new Map<string, number>();
    for (const e of list) {
      if (!plausibleTs(e.ts)) continue;
      for (const cp of CHECKPOINTS) {
        if (cp.events.includes(e.event)) checkpointTs.set(cp.step, e.ts);
      }
      for (const s of START_EVENTS) {
        if (e.event === s && !checkpointTs.has('start')) checkpointTs.set('start', e.ts);
      }
    }

    const steps: CardFlow['steps'] = {
      workS: null, pushS: null, buildS: null, deployS: null, demoS: null, mergeS: null, finalS: null,
    };
    let prev = checkpointTs.get('start') ?? first;
    for (const cp of CHECKPOINTS) {
      const at = checkpointTs.get(cp.step);
      if (at !== undefined) {
        // Out-of-order checkpoints (duplicate land attempts, cross-source clock
        // skew) make the boundary untrustworthy — render null, never negative.
        steps[cp.step] = at >= prev ? Math.round((at - prev) / 1000) : null;
        prev = at;
      }
    }

    const errors = list
      .filter((e) => isErrorEvent(e.event))
      .map((e) => ({ ts: e.ts, event: e.event, detail: e.detail ?? '' }));
    for (const e of errors) {
      classCounts.set(e.event, (classCounts.get(e.event) ?? 0) + 1);
      errorEvents++;
    }
    if (errors.length > 0) withErrors++;

    const landed = list.some((e) => LANDED_EVENTS.includes(e.event));
    if (landed) landedCount++;

    cards.push({
      card,
      role: list.find((e) => e.role)?.role ?? 'unknown',
      landed,
      lastEventTs: last,
      cycleS,
      steps,
      errors,
    });
  }

  // Sort newest-first by the (plausible) last event already computed per card,
  // so a poison-future timestamp can't jump a stale card to the top (#3397).
  cards.sort((a, b) => b.lastEventTs - a.lastEventTs);

  const errorClasses = [...classCounts.entries()]
    .map(([event, count]) => ({ event, count }))
    .sort((a, b) => b.count - a.count || a.event.localeCompare(b.event));

  const landedCycles = cards.filter((c) => c.landed).map((c) => c.cycleS).sort((a, b) => a - b);
  const cycleStats = {
    landedCards: landedCycles.length,
    medianS: percentile(landedCycles, 50),
    avgS: landedCycles.length ? Math.round(landedCycles.reduce((a, b) => a + b, 0) / landedCycles.length) : null,
    p90S: percentile(landedCycles, 90),
  };

  return {
    cards,
    errorClasses,
    totals: { cards: cards.length, landed: landedCount, withErrors, errorEvents },
    cycleStats,
  };
}

/** #3266 — the walk-away bar, derived (never declared). A card's land is:
 *  - a LAND when it reached card.accepted (outcome truth);
 *  - CLEAN when it landed in exactly ONE land run (one werk.land.started, no
 *    werk.land.failed, a werk.landed) — a retry or an orphan-completion
 *    (accepted without werk.landed) means a human touched it, so it is a land
 *    but not an unattended one. The streak counts consecutive clean lands in
 *    accept order; ready = streak >= K. K is named with Jeff, not assumed.
 */
export interface WalkAway {
  k: number;
  lands: number;
  cleanLands: number;
  currentStreak: number;
  ready: boolean;
}

export function deriveWalkAway(events: FlowEvent[], k: number): WalkAway {
  const byCard = new Map<number, FlowEvent[]>();
  for (const e of events) {
    if (typeof e.card_id !== 'number') continue;
    const list = byCard.get(e.card_id) ?? [];
    list.push(e);
    byCard.set(e.card_id, list);
  }

  // Per landed card: acceptance time + cleanliness.
  const landed: Array<{ acceptedAt: number; clean: boolean }> = [];
  for (const list of byCard.values()) {
    const accepted = list.filter((e) => e.event === 'card.accepted');
    if (accepted.length === 0) continue;
    const acceptedAt = Math.max(...accepted.map((e) => e.ts));
    const starts = list.filter((e) => e.event === 'werk.land.started').length;
    const fails = list.filter((e) => e.event === 'werk.land.failed').length;
    const landedEvt = list.some((e) => e.event === 'werk.landed');
    const clean = landedEvt && starts === 1 && fails === 0;
    landed.push({ acceptedAt, clean });
  }
  landed.sort((a, b) => a.acceptedAt - b.acceptedAt);

  let streak = 0;
  for (const l of landed) {
    streak = l.clean ? streak + 1 : 0;
  }

  return {
    k,
    lands: landed.length,
    cleanLands: landed.filter((l) => l.clean).length,
    currentStreak: streak,
    ready: streak >= k,
  };
}
