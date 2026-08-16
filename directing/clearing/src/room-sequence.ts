// @test-type: unit — pure sequence arithmetic over event tags; no relay, no clock.
/**
 * #3893 — show the hole.
 *
 * Replay (room-replay.ts) closes gaps caused by being disconnected. It cannot
 * tell you about a message that was never published, was refused by the relay,
 * or was dropped between the two. For that the room has to be able to COUNT.
 *
 * Jeff, 2026-08-15: "let's say I'm talking to another human and I hear one thing
 * they say and then I don't even know that they say two or three other things
 * and then I hear the fourth thing … The full conversation, not just a
 * scattershot from a shotgun." The italics belong on *I don't even know*. A
 * missing message that announces itself is a nuisance; a missing message that
 * doesn't is what makes a conversation untrustworthy.
 *
 * So every published note carries a per-author counter — tag `['seq', '<n>']`,
 * written by the publisher (chorus-hooks, #3893 Kade's leg). This module reads
 * those counters and answers one question per author: which numbers have we
 * never seen? The room renders the answer as a visible marker in the timeline
 * rather than logging it somewhere Jeff will never look.
 *
 * Deliberately NOT here: retry, backfill requests, dead-letter replay. A hole
 * that is displayed is already better than a hole that is silent, and machinery
 * to close it belongs behind evidence that displaying it isn't enough.
 */

/** The tag a publisher writes. One name, so a typo is a compile error here. */
export const SEQ_TAG = 'seq';

export interface SeqState {
  /** author → the highest seq seen. */
  highest: Map<string, number>;
  /** author → seqs seen (bounded by pruning below `highest - WINDOW`). */
  seen: Map<string, Set<number>>;
}

/** How far back a hole stays reportable. Beyond this the conversation has moved
 *  on and a marker is archaeology, not information. */
export const HOLE_WINDOW = 200;

export function emptySeqState(): SeqState {
  return { highest: new Map(), seen: new Map() };
}

/**
 * Read the sequence number off an event, or null.
 *
 * Null means "this publisher does not number its notes" — which is the state of
 * every note published before this card. Those notes must render normally: a
 * room that hid unnumbered messages would turn a missing FEATURE into missing
 * MESSAGES, the exact harm it exists to prevent.
 */
export function seqOf(ev: { tags?: string[][] }): number | null {
  const tag = ev.tags?.find((t) => t[0] === SEQ_TAG);
  // #3909 — `tag[1] === undefined` was unreachable: a tag that matched has at
  // least one element, and the string[] type says every element is a string. The
  // real check is whether the VALUE parses, done on the next line.
  if (!tag) return null;
  const n = Number(tag[1]);
  return Number.isInteger(n) && n >= 0 ? n : null;
}

/** Record one note's sequence. Unnumbered notes are ignored, not rejected. */
export function recordSeq(state: SeqState, author: string, seq: number | null): void {
  if (seq === null) return;
  const seen = state.seen.get(author) ?? new Set<number>();
  seen.add(seq);
  state.seen.set(author, seen);
  const prev = state.highest.get(author);
  const high = prev === undefined ? seq : Math.max(prev, seq);
  state.highest.set(author, high);
  // Prune below the window so a long-lived room does not grow a set per message.
  const floor = high - HOLE_WINDOW;
  if (floor > 0) for (const n of seen) if (n < floor) seen.delete(n);
}

/**
 * The numbers this author published that we have never seen, within the window.
 *
 * Note what is NOT reported: numbers above the highest we've seen. A message
 * still in flight is not a hole, and calling it one would make the marker cry
 * wolf on every live conversation until it was ignored — which is how a warning
 * becomes decoration.
 */
export function holesFor(state: SeqState, author: string): number[] {
  const high = state.highest.get(author);
  const seen = state.seen.get(author);
  if (high === undefined || seen === undefined) return [];
  const floor = Math.max(0, high - HOLE_WINDOW);
  const out: number[] = [];
  for (let n = floor; n < high; n++) if (!seen.has(n)) out.push(n);
  return out;
}

/** Every author's holes, authors with none omitted. */
export function allHoles(state: SeqState): Array<{ author: string; missing: number[] }> {
  const out: Array<{ author: string; missing: number[] }> = [];
  for (const author of state.highest.keys()) {
    const missing = holesFor(state, author);
    if (missing.length > 0) out.push({ author, missing });
  }
  return out;
}

/**
 * The line the room shows. Jeff reads this, so it says what happened and what
 * it means — not an error code, and never a silent skip.
 */
export function holeMarker(author: string, missing: number[]): string {
  if (missing.length === 0) return '';
  const list = missing.length <= 5
    ? missing.join(', ')
    : `${missing.slice(0, 5).join(', ')} … +${missing.length - 5} more`;
  const noun = missing.length === 1 ? 'message' : 'messages';
  return `${missing.length} ${noun} from ${author} never arrived (#${list})`;
}
