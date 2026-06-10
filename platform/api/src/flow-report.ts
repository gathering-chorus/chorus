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
  { step: 'demoS', events: ['werk.presented', 'demo.verdict'] },
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
    const first = list[0].ts;
    const last = list[list.length - 1].ts;

    // Last occurrence per checkpoint (retries supersede).
    const checkpointTs = new Map<string, number>();
    for (const e of list) {
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
      cycleS: Math.round((last - first) / 1000),
      steps,
      errors,
    });
  }

  cards.sort((a, b) => {
    const lastA = Math.max(...(byCard.get(a.card) ?? []).map((e) => e.ts));
    const lastB = Math.max(...(byCard.get(b.card) ?? []).map((e) => e.ts));
    return lastB - lastA;
  });

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
