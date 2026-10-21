// @domain: messages
/**
 * #4629 S3 — pulse delivers from Buzz. With PULSE_BUZZ_DELIVER=1, a nudge
 * whose sender signed its own room note (buzz=sender) is stored held, not
 * enqueued. It is released for delivery exactly once, by whichever comes
 * first: its room note (joined by trace, right key, same words), the sender
 * telling pulse its publish failed (release by trace), or the deliver-anyway
 * fallback (held longer than 30 s, Jeff 10-18 "deliver anyway"), which also
 * runs at boot so a restart mid-hold still delivers (Silas 06:42). Exactly
 * once comes from the store: only the caller whose guarded UPDATE moved the
 * row from held to pending enqueues it, across restarts.
 */
import { sqliteUtc } from './buzz-reader';

export interface HeldStore {
  holdForBuzz(id: number): void;
  releaseBuzzHeld(id: number): boolean;
  buzzHeldBefore(before: string): number[];
  getMessageEndsByTrace(trace: string): { id: number } | null;
}

export type ReleaseCause = 'room' | 'publish-failed' | 'fallback';

export interface BuzzDeliveryOptions {
  store: HeldStore;
  /** deliver one released row (the worker's enqueue) */
  deliver: (id: number) => void;
  onRelease: (id: number, cause: ReleaseCause) => void;
  holdMs?: number;
  now?: () => number;
}

export class BuzzDelivery {
  private readonly now: () => number;
  constructor(private readonly o: BuzzDeliveryOptions) { this.now = o.now ?? Date.now; }

  hold(id: number): void { this.o.store.holdForBuzz(id); }

  /** The row's room note arrived (already checked: right key, same words). */
  fromRoom(id: number): boolean { return this.release(id, 'room'); }

  /** The sender's publish failed: deliver now rather than after the fallback. */
  publishFailed(trace: string): boolean {
    const row = this.o.store.getMessageEndsByTrace(trace);
    return row ? this.release(row.id, 'publish-failed') : false;
  }

  /** Deliver every row held longer than the hold window. Returns how many. */
  sweep(): number {
    let n = 0;
    for (const id of this.o.store.buzzHeldBefore(sqliteUtc(this.now() - (this.o.holdMs ?? 30_000)))) {
      if (this.release(id, 'fallback')) n += 1;
    }
    return n;
  }

  private release(id: number, cause: ReleaseCause): boolean {
    if (!this.o.store.releaseBuzzHeld(id)) return false;
    this.o.onRelease(id, cause);
    this.o.deliver(id);
    return true;
  }
}
