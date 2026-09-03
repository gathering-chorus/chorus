/**
 * #4028 — role state derived from the streams. Nothing declared, nothing stored.
 *
 * Jeff, 2026-08-28: "i dont know why we cant rip out 'declared' role state and
 * have it be derived directly from streams." Before this, a role's state was a
 * file it wrote by hand (/tmp/claude-team-scan/<role>-declared.json); it
 * reverted to "unknown" overnight, said "idle" through six pipeline rounds,
 * and 142 cards of declaration never made it true.
 *
 * Agreed with Silas 2026-09-02 (chat wren-silas-1788369280):
 *   - DERIVE from the hooks daemon's stream. Every tool call a session makes
 *     already lands on the spine as hook.decision / context.inject.request with
 *     the role — the one source that sees a role WORKING, credential or not.
 *   - The security envelope ATTRIBUTES (verified webId); it never derives.
 *   - "blocked" is the one thing only the role knows. It is an EVENT
 *     (role.blocked, with a detail) and it expires the moment activity resumes.
 *   - Nothing is stored. The state is recomputed on every read, so nothing can
 *     revert and nothing can be forgotten.
 *
 * The card is not derived here: the board (WIP by owner) says what a role is
 * on; the streams say whether it is working it right now.
 */

export type RoleState = 'building' | 'waiting' | 'observing' | 'blocked' | 'idle';

/** One spine line, as parsed from ~/.chorus/chorus.log (JSONL). */
export interface SpineLine {
  timestamp: string;
  event: string;
  role?: string;
  card_id?: string | number;
  detail?: string;
  payload?: string;
}

export interface WipCardEntry {
  id: number;
  owner: string;
}

export interface StreamDeriveInput {
  role: string;
  /** Spine lines in the lookback (any role — the function filters). */
  events: SpineLine[];
  wipCards: WipCardEntry[];
  /** Milliseconds since epoch. */
  now: number;
  /** Activity window — AC1: the last 15 min. */
  windowMs?: number;
}

export interface DerivedRoleState {
  role: string;
  state: RoleState;
  card: number | null;
  multi_wip: boolean;
  wip_count: number;
  gemba: string | null;
  detail?: string;
  /** ISO timestamp of the role's latest activity event, if any in the window. */
  lastActivity: string | null;
  /** The role's latest event of any kind in the lookback. */
  lastEvent: string | null;
  source: 'streams';
}

export const ACTIVITY_WINDOW_MS = 15 * 60_000;
/** A presented demo waits up to an hour for its go before the role reads idle again. */
export const DEMO_LOOKBACK_MS = 60 * 60_000;
/** loom-gemba re-polls; an observation older than this is over. */
export const OBSERVING_WINDOW_MS = 10 * 60_000;

/**
 * Events that mean "this role did work". The hooks daemon writes the first
 * two on every tool call; the rest are the role's own verbs landing. NOT in
 * the set, on purpose: system.heartbeat, nudge.surfaced, observer.digest,
 * role.state.changed — a heartbeat is not work and a declaration is not work.
 */
export const ACTIVITY_EVENTS: ReadonlySet<string> = new Set([
  'hook.decision',
  'context.inject.request',
  'agent.action',
  'agent.activity',
  'reply.emitted',
  'reply.published',
  'model.write',
  'model.delete',
  'model.deployed',
  'commit.started',
  'commit.completed',
  'push.started',
  'push.completed',
  'test.started',
  'test.completed',
  'werk.phase',
  'card.pulled',
  'card.unpulled',
  'demo.presented',
  'gemba.observation.started',
  'nudge.emitted',
]);

const DEMO_CLOSERS: ReadonlySet<string> = new Set(['demo.go', 'demo.verdict', 'demo.no', 'card.accepted', 'card.unpulled']);

function ts(line: SpineLine): number {
  const t = Date.parse(line.timestamp);
  return Number.isFinite(t) ? t : 0;
}

function cardOf(line: SpineLine): string | null {
  if (line.card_id !== undefined) return String(line.card_id);
  const m = /(?:^|,)card(?:_id)?=(\d+)/.exec(line.payload ?? '');
  return m ? m[1] : null;
}

function gembaTarget(line: SpineLine): string | null {
  const m = /(?:^|,)(?:target|gemba)=([a-z]+)/i.exec(line.payload ?? '');
  return m ? m[1].toLowerCase() : null;
}

export function stateFromStreams(input: StreamDeriveInput): DerivedRoleState {
  const role = input.role.toLowerCase();
  const windowMs = input.windowMs ?? ACTIVITY_WINDOW_MS;
  const now = input.now;

  const mine = input.events
    .filter((e) => (e.role ?? '').toLowerCase() === role && ts(e) > 0 && ts(e) <= now)
    .sort((a, b) => ts(a) - ts(b));

  const myWip = input.wipCards.filter((c) => (c.owner || '').toLowerCase() === role);
  const card = myWip.length === 1 ? myWip[0].id : null;

  const base: DerivedRoleState = {
    role,
    state: 'idle',
    card,
    multi_wip: myWip.length > 1,
    wip_count: myWip.length,
    gemba: null,
    lastActivity: null,
    lastEvent: mine.length ? mine[mine.length - 1].event : null,
    source: 'streams',
  };

  const activity = mine.filter((e) => ACTIVITY_EVENTS.has(e.event) && now - ts(e) <= windowMs);
  const lastActivity = activity.length ? activity[activity.length - 1] : null;
  if (lastActivity) base.lastActivity = lastActivity.timestamp;

  // blocked — the one declared thing, as an event; the latest thing the role
  // did inside the window, and only until it does anything else.
  const blocked = [...mine].reverse().find((e) => e.event === 'role.blocked' && now - ts(e) <= windowMs);
  if (blocked && (!lastActivity || ts(lastActivity) <= ts(blocked))) {
    return { ...base, state: 'blocked', detail: blocked.detail ?? undefined };
  }

  // observing — loom-gemba polled recently.
  const gemba = [...mine].reverse().find((e) => e.event === 'gemba.observation.started' && now - ts(e) <= OBSERVING_WINDOW_MS);
  if (gemba) {
    return { ...base, state: 'observing', gemba: gembaTarget(gemba) };
  }

  // waiting — a demo this role presented that nobody has answered.
  const presented = [...mine].reverse().find((e) => e.event === 'demo.presented' && now - ts(e) <= DEMO_LOOKBACK_MS);
  if (presented) {
    const presentedCard = cardOf(presented);
    const closed = input.events.some((e) =>
      DEMO_CLOSERS.has(e.event) && ts(e) > ts(presented) && (presentedCard === null || cardOf(e) === presentedCard));
    if (!closed) return { ...base, state: 'waiting' };
  }

  if (lastActivity) return { ...base, state: 'building' };
  return base;
}
