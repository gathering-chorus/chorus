/* eslint-disable @typescript-eslint/require-await -- injected async doubles document the Promise contract */
// @test-type: unit — injected store/inject/spine, temp db; no live pulse, no real inject
//
// #4004 — delivery-worker sat at 73% function coverage with the SUPPRESSION
// branch (announceOrSuppress, lines 177-188) untested. That branch decides
// whether a duplicate machine-lane message reaches a terminal or is dropped as
// already-terminal. Untested, it can regress two ways Jeff feels directly:
// a suppressed row retried forever (noise), or a real message silently dropped.
import { DeliveryWorker, type DeliveryRow } from './delivery-worker';
import { MessageStore } from './store';
import * as fs from 'fs';
import * as path from 'path';

const TEST_DB = path.join(__dirname, '..', 'test-delivery-suppression-4004.db');
let store: MessageStore;

beforeEach(() => {
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
  store = new MessageStore(TEST_DB);
});
afterEach(() => {
  try { store.close(); } catch { /* already closed */ }
  if (fs.existsSync(TEST_DB)) fs.unlinkSync(TEST_DB);
});

const rowFor = (id: number, content: string): DeliveryRow =>
  ({ id, from: 'system', to: 'silas', content, delivery_attempts: 0 });

describe('#4004 delivery suppression branch', () => {
  it('a repeated machine-lane message is SUPPRESSED, marked handled, and never injected twice', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    let injects = 0;
    const worker = new DeliveryWorker(
      store,
      async () => { injects += 1; return { rc: 0, stderr: '' }; },
      async (event, fields) => { events.push({ event, fields }); },
      [1, 2],
      async () => { /* no real sleep */ },
    );
    const body = '[mcp.error] mcp.transport.error POST /mcp status=400';
    const first = store.sendNudge('system', 'silas', body);
    const second = store.sendNudge('system', 'silas', body);

    await worker.enqueue(rowFor(first, body));
    await worker.enqueue(rowFor(second, body));

    const suppressed = events.filter(e => e.event === 'terminal.suppressed');
    expect(suppressed.length).toBeGreaterThan(0);
    // handled, not failed — the fold stays honest and nothing retries forever
    expect(store.getDeliveryRecord(second).delivery_status).toBe('delivered');
    // and the suppressed row was never pushed to a terminal
    expect(injects).toBe(1);
  });

  it('the suppression event names its reason and lane — a drop that cannot explain itself is not allowed', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const worker = new DeliveryWorker(
      store, async () => ({ rc: 0, stderr: '' }),
      async (event, fields) => { events.push({ event, fields }); },
      [1, 2], async () => {},
    );
    const body = '[mcp.error] duplicate machine line';
    const a = store.sendNudge('system', 'silas', body);
    const b = store.sendNudge('system', 'silas', body);
    await worker.enqueue(rowFor(a, body));
    await worker.enqueue(rowFor(b, body));

    const s = events.find(e => e.event === 'terminal.suppressed');
    expect(s).toBeDefined();
    expect(s!.fields.reason).toBeTruthy();
    expect(s!.fields.lane).toBeTruthy();
    expect(s!.fields.to).toBe('silas');
  });

  it('NEGATIVE PROOF: a distinct human message is ANNOUNCED, not suppressed (#3734)', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    let injects = 0;
    const worker = new DeliveryWorker(
      store,
      async () => { injects += 1; return { rc: 0, stderr: '' }; },
      async (event, fields) => { events.push({ event, fields }); },
      [1, 2], async () => {},
    );
    const one = store.sendNudge('wren', 'silas', 'a real question about #4004');
    const two = store.sendNudge('wren', 'silas', 'a different real question');
    await worker.enqueue({ id: one, from: 'wren', to: 'silas', content: 'a real question about #4004', delivery_attempts: 0 });
    await worker.enqueue({ id: two, from: 'wren', to: 'silas', content: 'a different real question', delivery_attempts: 0 });

    expect(events.filter(e => e.event === 'terminal.suppressed')).toHaveLength(0);
    expect(injects).toBe(2);
  });
});
