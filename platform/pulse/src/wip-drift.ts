/**
 * wip-drift.ts — #3879: the check that ends "finish ur card".
 *
 * Jeff has chased card-finishing ≥6 times since 2026-04-24. DEC-1571 states
 * the rule in prose; nothing red fired on drift, so the nag repeated. This
 * watcher compares each WIP card's owner activity against card-attributed
 * activity: an owner active DRIFT_WINDOW_MS+ while the card saw nothing emits
 * `wip.drift` and nudges the ROLE. Only a second consecutive window escalates
 * to Jeff — he is the LAST resort, not the detector.
 *
 * Doc-comment ledger (the check knows what it exists to end): "did u finish
 * ur card" 2026-05-21 · "finish the card" 06-06 · 06-19 ×2 roles · 07-22 ·
 * "finish 3799" + "/fuc (finish ur card)" 08-14.
 *
 * Pure core (detectWipDrift) + a thin watch loop wired at pulse boot with
 * injectable reads/emits — the reply-gap (#3864) pattern.
 */

export const DRIFT_WINDOW_MS = 4 * 60 * 60 * 1000;

/** Owner counts as "active" if they emitted anything this recently. */
export const ROLE_ACTIVE_MS = 15 * 60 * 1000;

export interface DriftInput {
  role: string;
  cardId: number;
  /** newest spine event from this role, any kind (heartbeat, tool, reply) */
  lastRoleActivityMs: number;
  /** newest CARD-ATTRIBUTED event: werk-commit, pipeline run, card comment.
   *  0 = never OBSERVED — which is unknown, not "idle since epoch". */
  lastCardActivityMs: number;
  nowMs: number;
  /** when this card last fired wip.drift, if the card is untouched since */
  priorDriftAtMs: number | null;
  /** when observation of this card began (watcher start / card first seen).
   *  First live firing (2026-08-14 18:32, on its own author): the spine tail
   *  is ~6 min deep while the window is 4h — "no card event in view" is NOT
   *  "no card event in 4h". The check must not fire until it has WATCHED a
   *  full window; idle counts from observation start, never epoch (the
   *  "496319h" nudge). */
  observedSinceMs: number;
}

export interface Drift {
  role: string;
  cardId: number;
  idleCardMs: number;
  /** first window nudges the ROLE; the second consecutive one reaches Jeff */
  escalateToJeff: boolean;
}

/**
 * Pure: one WIP card in, a drift verdict out.
 * - The ROLE must be currently active — a declared-idle or absent role is not
 *   drifting, it is idle (the andon covers that; wrong-idle is #3882's beat).
 * - The CARD must be untouched for the full window.
 * - A card touch after a prior drift resets the ladder (priorDrift ignored
 *   when card activity is newer than it).
 */
export function detectWipDrift(i: DriftInput): Drift | null {
  const roleActive = i.nowMs - i.lastRoleActivityMs <= ROLE_ACTIVE_MS;
  if (!roleActive) return null;
  // Idle counts from the newest of (card activity, observation start): a
  // window we did not watch is UNKNOWN, never idle. This both prevents the
  // fire-on-first-sight false positive and caps the reported number at the
  // observed span — no more since-epoch hours.
  const idleFrom = Math.max(i.lastCardActivityMs, i.observedSinceMs);
  const idleCardMs = i.nowMs - idleFrom;
  if (idleCardMs <= DRIFT_WINDOW_MS) return null;
  const priorStands = i.priorDriftAtMs !== null && i.priorDriftAtMs > i.lastCardActivityMs;
  return {
    role: i.role,
    cardId: i.cardId,
    idleCardMs,
    escalateToJeff: priorStands,
  };
}

// ── service wiring (thin; injectable like reply-gap #3864) ──────────────────

export interface WipCard { id: number; owner: string }
export type ReadWip = () => Promise<WipCard[]>;
export type ReadActivity = (role: string, cardId: number) => Promise<{ lastRoleActivityMs: number; lastCardActivityMs: number }>;
export type EmitDrift = (d: Drift) => Promise<void>;

/**
 * Watch loop: each tick, read WIP cards, evaluate each owner, emit each drift
 * once per window (the prior-drift ladder is in-memory; a restart starts the
 * ladder over — acceptable: it under-escalates, never over). Returns stop().
 */
export function startWipDriftWatch(
  readWip: ReadWip,
  readActivity: ReadActivity,
  emitDrift: EmitDrift,
  intervalMs = 10 * 60 * 1000,
  now: () => number = Date.now,
): () => void {
  const priorDrift = new Map<string, number>(); // `${role}:${card}` → firedAt
  // #3880 fix (first live firing was a false positive on its own author):
  // the spine tail covers minutes, the window is hours — so ACCUMULATE the
  // newest-seen timestamps across ticks instead of trusting one shallow read,
  // and date observation per card so the first window must be genuinely
  // WATCHED before anything fires.
  const seenCard = new Map<string, number>();   // key → newest card event ever seen
  const seenRole = new Map<string, number>();   // role → newest role event ever seen
  const observedSince = new Map<string, number>(); // key → when we started watching
  const timer = setInterval(() => {
    void (async () => {
      try {
        for (const c of await readWip()) {
          const role = c.owner.toLowerCase();
          const a = await readActivity(role, c.id);
          const key = `${role}:${c.id}`;
          if (!observedSince.has(key)) observedSince.set(key, now());
          seenCard.set(key, Math.max(seenCard.get(key) ?? 0, a.lastCardActivityMs));
          seenRole.set(role, Math.max(seenRole.get(role) ?? 0, a.lastRoleActivityMs));
          const d = detectWipDrift({
            role, cardId: c.id, nowMs: now(),
            lastRoleActivityMs: seenRole.get(role) ?? 0,
            lastCardActivityMs: seenCard.get(key) ?? 0,
            priorDriftAtMs: priorDrift.get(key) ?? null,
            observedSinceMs: observedSince.get(key) ?? now(),
          });
          if (!d) { if (a.lastCardActivityMs > (priorDrift.get(key) ?? 0)) priorDrift.delete(key); continue; }
          const fired = priorDrift.get(key) ?? 0;
          if (now() - fired < DRIFT_WINDOW_MS) continue; // one nudge per window
          priorDrift.set(key, now());
          await emitDrift(d);
        }
      } catch (e) {
        console.error('[wip-drift] scan failed:', e instanceof Error ? e.message : e);
      }
    })();
  }, intervalMs);
  return () => clearInterval(timer);
}
