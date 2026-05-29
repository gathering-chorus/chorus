/**
 * Delivery Worker (#2727 AC2)
 *
 * Pulse owns nudge delivery. Worker drives the transition from messages.db
 * row → chorus-inject call → spine event → delivered|failed terminal state.
 *
 * Constructor takes injectable runInject + emitSpine so tests mock without
 * spawning real chorus-inject or writing to chorus.log.
 *
 * Per-receiver-role serial FIFO (AC10): same receiver = sequential via a
 * Promise chain Map. Different receivers = parallel.
 *
 * Backoff (AC2): [250ms, 500ms, 1s, 2s, 5s] for transient failures.
 * Permanent reasons (tcc-denied, no-window-found, window-ambiguous,
 * encoding-error) skip retry and mark failed immediately.
 *
 * Ordering (AC11): emit nudge.surfaced BEFORE markDelivered, both inside
 * a single try block so partial failures don't strand state.
 */

import { MessageStore } from './store';

// #3125: `deferred` signals the target is a host osascript can't reach
// safely (VS Code) — runInject declined to push and the nudge should be
// handed to the inbox/fold instead of surfaced or failed.
export type InjectResult = { rc: number; stderr: string; deferred?: boolean; deferReason?: string };
export type RunInject = (to: string, content: string) => Promise<InjectResult>;
export type EmitSpine = (event: string, fields: Record<string, unknown>) => Promise<void>;

export interface DeliveryRow {
  id: number;
  from: string;
  to: string;
  content: string;
  delivery_attempts: number;
  // #2765: trace_id correlation key. UUIDv7 minted at sender. Null for
  // legacy rows or sender that didn't propagate the header (worker mints
  // a fallback when absent so every event still carries one).
  trace_id?: string | null;
}

export const DEFAULT_BACKOFF_MS = [250, 500, 1000, 2000, 5000];

// Per Kade gemba review 2026-05-07: chorus-inject classifies exactly ONE
// failure structurally — "no claude window found" (lib.rs:105). tcc-denied
// comes from osascript's stderr verbatim (locale/version-unstable), and
// window-ambiguous + encoding-error have no concrete signal at all. Keep
// the contract honest: only no-window-found is reliably permanent. Everything
// else stays transient and retries until exhausted. If chorus-inject adds
// structured error tags later, this set expands to match.
export const PERMANENT_REASONS = new Set([
  'no claude window found',
]);

/**
 * Classify a chorus-inject result as 'success' | 'permanent' | 'transient'.
 * Permanent: only failures the binary structurally classifies (today: window-not-found).
 * Transient: anything else with non-zero rc — retry until exhausted.
 */
export function classifyInjectResult(r: InjectResult): { kind: 'success' | 'permanent' | 'transient'; reason: string } {
  if (r.rc === 0) return { kind: 'success', reason: 'ok' };
  const stderr = (r.stderr || '').toLowerCase();
  for (const reason of PERMANENT_REASONS) {
    if (stderr.includes(reason)) {
      return { kind: 'permanent', reason: 'no-window-found' };
    }
  }
  return { kind: 'transient', reason: stderr.split('\n')[0].slice(0, 120) || `rc=${r.rc}` };
}

export type SelfTest = () => Promise<InjectResult>;

export class DeliveryWorker {
  private chains: Map<string, Promise<void>> = new Map();

  constructor(
    private store: MessageStore,
    private runInject: RunInject,
    private emitSpine: EmitSpine,
    private backoffMs: number[] = DEFAULT_BACKOFF_MS,
    private sleep: (ms: number) => Promise<void> = (ms) => new Promise(r => setTimeout(r, ms)),
    private selfTest: SelfTest = async () => ({ rc: 0, stderr: '' }),
  ) {}

  /**
   * Boot smoke test (AC12). Pulse calls this before app.listen.
   * Failure throws — caller should exit non-zero before opening the listener.
   */
  async startupSmoke(): Promise<void> {
    const result = await this.selfTest();
    if (result.rc !== 0) {
      await this.emitSpine('nudge.health.smoke_failed', {
        rc: result.rc,
        stderr: result.stderr,
      });
      throw new Error(`startup smoke failed: rc=${result.rc} stderr=${result.stderr}`);
    }
    await this.emitSpine('nudge.health.smoke_ok', {});
  }

  /**
   * Enqueue a row for delivery. Returns a promise that resolves when
   * the row reaches terminal state (delivered or failed).
   *
   * Per-receiver-role serial: same `to` = sequential via promise chain.
   * Different `to` = parallel (independent chains).
   */
  enqueue(row: DeliveryRow): Promise<void> {
    const prev = this.chains.get(row.to) || Promise.resolve();
    const next = prev.then(() => this.deliverOne(row)).catch(() => { /* swallow so chain continues */ });
    this.chains.set(row.to, next);
    return next;
  }

  /**
   * Scan messages.db for pending deliveries and enqueue each.
   * Called at pulse boot for restart-requeue (AC8).
   */
  async scanAndRequeue(): Promise<void> {
    const pending = this.store.getPendingDeliveries();
    for (const row of pending) {
      this.enqueue(row);
    }
  }

  /**
   * Deliver a single row with backoff retry. Permanent reasons skip retry.
   * Emits nudge.surfaced or nudge.surface.failed; updates row terminal state.
   */
  private async deliverOne(row: DeliveryRow): Promise<void> {
    const maxAttempts = this.backoffMs.length + 1;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      const result = await this.runInject(row.to, row.content);
      const classified = classifyInjectResult(result);

      // #2765 — trace_id propagated to every spine event in lifecycle
      const traceFields = row.trace_id ? { trace_id: row.trace_id } : {};

      // #3125 AC6/AC4: hand off to the inbox/fold instead of pushing for a
      // VS-Code-hosted target (runInject declined — the session isn't a
      // Terminal tab osascript can address). Recoverable, NOT a failure —
      // failing would drop it from the fold. Emit nudge.deferred — deliberately
      // NOT nudge.surfaced (it wasn't pushed) and NOT nudge.surface.failed (the
      // fold subtracts surface.failed, which would drop it). The spine fold
      // (emitted − surfaced − surface.failed) thus keeps it pending and the
      // receiver's UserPromptSubmit drain injects it inline. markDelivered makes
      // the messages.db row terminal so it isn't re-scanned forever.
      //
      // #3128: the focus-gate-miss deferral is GONE. chorus-inject no longer
      // refuses on frontmost-app — it ALWAYS WAKES (activates Terminal on a tty
      // match), so a focus-gate-miss stderr can no longer occur. Nudges deliver,
      // they don't defer behind a focus check.
      if (result.deferred) {
        await this.emitSpine('nudge.deferred', {
          ...traceFields,
          id: row.id,
          from: row.from,
          to: row.to,
          attempt,
          reason: result.deferReason || 'inbox',
        });
        this.store.markDelivered(row.id);
        return;
      }

      if (classified.kind === 'success') {
        try {
          await this.emitSpine('nudge.surfaced', {
            ...traceFields,
            id: row.id,
            from: row.from,
            to: row.to,
            attempt,
          });
          this.store.markDelivered(row.id);
        } catch (e) {
          throw e;
        }
        return;
      }

      if (classified.kind === 'permanent') {
        await this.emitSpine('nudge.surface.failed', {
          ...traceFields,
          id: row.id,
          from: row.from,
          to: row.to,
          attempt,
          reason: classified.reason,
          permanent: true,
        });
        this.store.markFailed(row.id, classified.reason);
        return;
      }

      // transient — emit per-attempt failure event, retry if attempts left
      await this.emitSpine('nudge.surface.failed', {
        ...traceFields,
        id: row.id,
        from: row.from,
        to: row.to,
        attempt,
        reason: classified.reason,
        permanent: false,
      });

      if (attempt >= maxAttempts) {
        this.store.markFailed(row.id, classified.reason);
        return;
      }

      const backoffIdx = attempt - 1;
      // eslint-disable-next-line security/detect-object-injection -- backoffIdx is a bounded attempt counter, not external input
      await this.sleep(this.backoffMs[backoffIdx]);
    }
  }
}
