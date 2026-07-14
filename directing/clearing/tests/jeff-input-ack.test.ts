// @test-type: unit — pure contract module (processJeffInput), injected fakes, no I/O.
// #3646 — TDD: the ack contract that fixes the works-once bug.
// The client's "Sent" must mean accepted+persisted (ingested), never "every
// terminal hand-off finished" — the old inline handler awaited sequential
// 5s-bounded deliveries before acking while the UI timed out at 3s.
import { processJeffInput, DeliveryStatus, JeffInputDeps } from '../src/jeff-input';

function makeDeps(overrides: Partial<JeffInputDeps> = {}): {
  deps: JeffInputDeps;
  calls: { ingested: unknown[]; delivered: string[]; statuses: DeliveryStatus[] };
} {
  const calls = { ingested: [] as unknown[], delivered: [] as string[], statuses: [] as DeliveryStatus[] };
  const deps: JeffInputDeps = {
    ingest: (m) => calls.ingested.push(m),
    deliver: async (t) => { calls.delivered.push(t); return null; },
    targetsOf: () => ['wren'],
    now: () => '2026-07-14T12:00:00.000Z',
    onDeliveryStatus: (s) => calls.statuses.push(s),
    ...overrides,
  };
  return { deps, calls };
}

describe('#3646 jeff-input ack contract', () => {
  it('empty text is refused without ingesting', async () => {
    const { deps, calls } = makeDeps();
    const acks: unknown[] = [];
    await processJeffInput(deps, { text: '   ', from: 'jeff' }, (r) => acks.push(r));
    expect(acks).toEqual([{ ok: false, error: 'empty' }]);
    expect(calls.ingested).toHaveLength(0);
  });

  it('acks ok immediately after ingest, BEFORE any delivery resolves', async () => {
    let releaseDelivery!: () => void;
    const gate = new Promise<void>((res) => { releaseDelivery = res; });
    const order: string[] = [];
    const { deps } = makeDeps({
      ingest: () => order.push('ingest'),
      deliver: async () => { await gate; order.push('deliver-done'); return null; },
    });
    const done = processJeffInput(deps, { text: 'hello', from: 'jeff' }, () => order.push('ack'));
    // ack must have fired synchronously after ingest — delivery still pending
    expect(order).toEqual(['ingest', 'ack']);
    releaseDelivery();
    await done;
    expect(order).toEqual(['ingest', 'ack', 'deliver-done']);
  });

  it('a failed hand-off never retracts the ack; it surfaces as a delivery status', async () => {
    const acks: Array<{ ok: boolean }> = [];
    const { deps, calls } = makeDeps({
      deliver: async () => 'pulse 503: worker down',
    });
    const statuses = await processJeffInput(deps, { text: 'hi', from: 'jeff' }, (r) => acks.push(r));
    expect(acks).toEqual([{ ok: true }]);
    expect(statuses).toEqual([{ target: 'wren', ok: false, error: 'pulse 503: worker down' }]);
    expect(calls.statuses).toEqual(statuses);
  });

  it('multi-target deliveries run in parallel, one ack total', async () => {
    const started: string[] = [];
    let releaseAll!: () => void;
    const gate = new Promise<void>((res) => { releaseAll = res; });
    const acks: unknown[] = [];
    const { deps } = makeDeps({
      targetsOf: () => ['wren', 'silas', 'kade'],
      deliver: async (t) => { started.push(t); await gate; return null; },
    });
    const done = processJeffInput(deps, { text: '@wren @silas @kade go', from: 'jeff' }, (r) => acks.push(r));
    await Promise.resolve(); // let the parallel map start
    expect(started).toEqual(['wren', 'silas', 'kade']); // all started before ANY resolved
    releaseAll();
    const statuses = await done;
    expect(statuses).toHaveLength(3);
    expect(acks).toHaveLength(1);
  });

  it('mixed outcomes report per-target', async () => {
    const { deps } = makeDeps({
      targetsOf: () => ['wren', 'kade'],
      deliver: async (t) => (t === 'kade' ? 'timeout' : null),
    });
    const statuses = await processJeffInput(deps, { text: '@wren @kade hi', from: 'mark' });
    expect(statuses).toEqual([
      { target: 'wren', ok: true },
      { target: 'kade', ok: false, error: 'timeout' },
    ]);
  });
});
