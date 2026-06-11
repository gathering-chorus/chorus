// #3357 — ONE announce boundary for everything that reaches a terminal.
//
// Evidence (2026-06-11): 42% of Wren's inbound was noise-lane — 8 system alerts
// (the same event-loop incident announced 7× across 3 terminals = 20+ on Jeff's
// day), 7 raw mcp.tool.error announcements that were typed REFUSALS working as
// designed (cards_tag → "refused: use-cards-set"), and 6 self-echoes. The
// machine lane was also UNMETERED — 12 alerts hit terminals, 2 nudge.emitted
// events existed.
//
// The boundary types every delivery (the refusal-taxonomy treatment applied to
// announcements), dedupes machine repeats by CLASS SIGNATURE with a cooldown
// (numbers/timestamps stripped so "blocked 8000ms" and "blocked 6083ms" are one
// incident), kills self-echo, and never silently eats a first occurrence — a
// real incident always announces once, with the suppressed count carried on the
// next announcement so information is delayed, never lost.
//
// Pure module: the caller (delivery-worker) owns the state and the spine emits
// (terminal.announced / terminal.suppressed — the metering AC).

export type Lane = 'role' | 'machine';
export type Cls = 'message' | 'alert' | 'refusal' | 'error';

export interface Classification {
  lane: Lane;
  cls: Cls;
}

export interface Decision {
  deliver: boolean;
  cls: Cls;
  lane: Lane;
  suppressReason?: 'self-echo' | 'benign-refusal' | 'dup-class';
  /** running count of suppressions for this signature within the open window */
  suppressedCount?: number;
  /** on a delivery that follows suppressions: how many repeats were absorbed */
  suppressedSinceLast?: number;
}

/** Senders that are machines, not roles. Extend deliberately — a new machine
 *  sender joining the nudge surface should be a conscious decision. */
const MACHINE_SENDERS = new Set(['system', 'chorus-mcp', 'chorus-api', 'pulse']);

/** Typed-refusal recognizer — KNOWN CLASSES ONLY (the #3334 benign-list,
 *  here as code). A bare /refused:/ match would eat real errors ("connection
 *  refused: timeout" — cold-eyes catch on this card), so membership in the
 *  finite refusal taxonomy is required; an unknown refused:-class FAILS OPEN
 *  to class=error and DELIVERS. Adding a class here is a conscious edit. */
export const BENIGN_REFUSAL_CLASSES = new Set([
  'use-cards-set', 'no-status-changes', 'wrong-status', 'wrong-owner',
  'no-werk', 'no-ac', 'dirty-floor-inputs', 'ceremony-rejected',
  'gates-missing', 'no-open-pr', 'no-approval', 'card-not-found',
  'ac-missing', 'experience-missing', 'move-fail', 'branch-fail',
  'werk-dirty', 'werk-not-initialized', 'werk-corrupt',
]);
const REFUSAL_RE = /\brefused:\s*([a-z][a-z0-9-]*)/i;
function isBenignRefusal(content: string): boolean {
  const m = REFUSAL_RE.exec(content);
  return m !== null && BENIGN_REFUSAL_CLASSES.has(m[1].toLowerCase());
}

export function classify(from: string, _to: string, content: string): Classification {
  const lane: Lane = MACHINE_SENDERS.has(from) ? 'machine' : 'role';
  if (lane === 'role') return { lane, cls: 'message' };
  if (isBenignRefusal(content)) return { lane, cls: 'refusal' };
  if (/mcp\.tool\.error|error|fail|ENOENT|exception/i.test(content)) return { lane, cls: 'error' };
  return { lane, cls: 'alert' };
}

/** Class signature: the incident identity with volatile parts (numbers,
 *  timestamps) stripped, truncated so trailing variance can't split one
 *  incident into many. "blocked 8000ms at 13:30" == "blocked 6083ms at 11:06". */
export function signature(content: string): string {
  return content
    .replace(/\d+/g, 'N')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 48)
    .toLowerCase();
}

/** Machine-lane cooldown: one announcement per (signature) per window per
 *  recipient-set. 30min — long enough to absorb today's 22-minute alert cadence,
 *  short enough that an ongoing incident re-surfaces twice an hour. */
export const MACHINE_COOLDOWN_MS = 30 * 60_000;

// DECIDED (Jeff's 20×3 datum, 2026-06-11): the window is keyed by signature
// ONLY — recipient-agnostic. One incident announces ONCE ANYWHERE, because the
// same alert fanned to three terminals all land in front of the same human.
// If per-recipient once-each is ever wanted, key by (signature, to) here.
interface WindowEntry {
  openedAt: number;
  suppressed: number;
}

export class AnnounceState {
  /** keyed by signature (recipient-agnostic on purpose: the same incident
   *  fanned to 3 terminals is ONE incident — Jeff's 20×3 datum) */
  windows: Map<string, WindowEntry> = new Map();
}

export function decide(
  from: string,
  to: string,
  content: string,
  nowMs: number,
  state: AnnounceState,
): Decision {
  const { lane, cls } = classify(from, to, content);

  // 1) self-echo: a sender never receives its own send. Applies to all lanes.
  if (from === to) {
    return { deliver: false, cls, lane, suppressReason: 'self-echo' };
  }

  // 2) role lane always delivers — peers talking is the system working.
  //    (Exact-duplicate double-fires are handled upstream by #3335.)
  if (lane === 'role') {
    return { deliver: true, cls, lane };
  }

  // 3) typed refusals are benign control flow — already handled by the caller.
  if (cls === 'refusal') {
    return { deliver: false, cls, lane, suppressReason: 'benign-refusal' };
  }

  // 4) machine alerts/errors: dedupe by class signature with cooldown.
  const sig = signature(content);
  const win = state.windows.get(sig);
  if (win && nowMs - win.openedAt < MACHINE_COOLDOWN_MS) {
    win.suppressed += 1;
    return {
      deliver: false,
      cls,
      lane,
      suppressReason: 'dup-class',
      suppressedCount: win.suppressed,
    };
  }
  const suppressedSinceLast = win ? win.suppressed : 0;
  state.windows.set(sig, { openedAt: nowMs, suppressed: 0 });
  // prune long-dead windows so the map can't grow unbounded
  for (const [k, w] of state.windows) {
    if (nowMs - w.openedAt > 4 * MACHINE_COOLDOWN_MS) state.windows.delete(k);
  }
  return { deliver: true, cls, lane, suppressedSinceLast };
}
