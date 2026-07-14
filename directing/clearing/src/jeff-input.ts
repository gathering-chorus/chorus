/**
 * jeff-input.ts — #3646: the jeff-message orchestration, extracted so the ack
 * contract is unit-pinned.
 *
 * THE CONTRACT (the works-once bug's root): the client acknowledges a send as
 * accepted the moment it is INGESTED (persisted to the room + receipts) — never
 * after terminal delivery. The old inline handler awaited every per-target
 * hand-off (5s abort each, sequential) BEFORE acking, while the UI gave up at
 * 3s — so boundary-timed sends showed "Failed", restored the text, and invited
 * duplicate resends for messages that had actually landed. Remote origins added
 * round-trip time to the ack path and hit the boundary more often.
 *
 * Post-#3646: ingest → ack ok → deliveries run in PARALLEL, and each target's
 * outcome is reported via onDeliveryStatus (the UI renders failures visibly —
 * a failed hand-off is information, not a retraction of the accepted send).
 */

export interface JeffInputMessage {
  from: string;
  text: string;
  ts: string;
  type: 'jeff-input';
}

export interface DeliveryStatus {
  target: string;
  ok: boolean;
  error?: string;
}

export interface JeffInputDeps {
  /** Persist into the room (messageRouter.ingest) — the ack fires right after this. */
  ingest: (msg: JeffInputMessage) => void;
  /** Hand one target's copy to the pulse worker. Returns null on success, reason on failure. */
  deliver: (target: string) => Promise<string | null>;
  /** Resolve @mentions / default routing to the target role list. */
  targetsOf: (text: string) => string[];
  now: () => string;
  /** Per-target outcome, emitted as each hand-off settles (UI shows failures). */
  onDeliveryStatus?: (status: DeliveryStatus) => void;
}

export async function processJeffInput(
  deps: JeffInputDeps,
  data: { text: string; from: string },
  ack?: (result: { ok: boolean; error?: string }) => void,
): Promise<DeliveryStatus[]> {
  const text = (data.text || '').trim();
  if (!text) {
    ack?.({ ok: false, error: 'empty' });
    return [];
  }

  deps.ingest({ from: data.from, text, ts: deps.now(), type: 'jeff-input' });
  // Accepted + persisted = sent. Terminal delivery reports separately.
  ack?.({ ok: true });

  const targets = deps.targetsOf(text);
  return Promise.all(
    targets.map(async (target): Promise<DeliveryStatus> => {
      const error = await deps.deliver(target);
      const status: DeliveryStatus = { target, ok: error === null, ...(error ? { error } : {}) };
      deps.onDeliveryStatus?.(status);
      return status;
    }),
  );
}
