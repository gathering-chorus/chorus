// @test-type: unit — pulse worker and verdict on a temp db; inject faked
// @domain: messages
// @card: #4889
// @owner: silas
// #4889 (folded #4893; Jeff 07:48 "break decisively rather than branch off to some old
// path") — src/delivery-worker: a registered pane that is gone is one loud not-delivered,
// never retried and never guessed elsewhere; src/service deliveryVerdict tells the sender.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { MessageStore } from './store';
import { DeliveryWorker, classifyInjectResult } from './delivery-worker';
import { deliveryVerdict } from './service';

const tmpDb = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'dead-pane-4889-')), 'messages.db');
const noSleep = () => Promise.resolve();

describe('#4889 a dead pane is not-delivered, once, with its reason', () => {
  it('"can\'t find pane" is permanent and names the pane', () => {
    expect(classifyInjectResult({ rc: 1, stderr: "0:686: execution error: can't find pane: %12 (1)" }))
      .toEqual({ kind: 'permanent', reason: 'dead-pane %12' });
  });

  it('NEGATIVE PROOF: another failure is still transient (retried)', () => {
    expect(classifyInjectResult({ rc: 1, stderr: 'osascript timed out' }).kind).toBe('transient');
  });

  it('the worker tries the dead pane once and marks the row failed with the reason', async () => {
    const store = new MessageStore(tmpDb());
    let tries = 0;
    const inject = () => { tries += 1; return Promise.resolve({ rc: 1, stderr: "can't find pane: %12" }); };
    const w = new DeliveryWorker(store, inject, () => Promise.resolve(), [1, 1, 1, 1, 1], noSleep);
    const id = store.sendNudge('wren', 'abby-normal', 'are you there');
    await w.enqueue({ id, from: 'wren', to: 'abby-normal', content: 'are you there', delivery_attempts: 0 });
    expect(tries).toBe(1);
    expect(deliveryVerdict(store.getDeliveryRecord(id))).toEqual({ delivery: 'not-delivered', reason: expect.stringContaining('dead-pane %12') });
    store.close();
  });

  it('NEGATIVE PROOF: a pane that answers is delivered', async () => {
    const store = new MessageStore(tmpDb());
    const w = new DeliveryWorker(store, () => Promise.resolve({ rc: 0, stderr: '' }), () => Promise.resolve(), [1], noSleep);
    const id = store.sendNudge('wren', 'kade', 'hi');
    await w.enqueue({ id, from: 'wren', to: 'kade', content: 'hi', delivery_attempts: 0 });
    expect(deliveryVerdict(store.getDeliveryRecord(id))).toEqual({ delivery: 'delivered' });
    store.close();
  });
});

describe('#4889 deliveryVerdict: what the sender is told', () => {
  it('maps each row state, and never calls a pending row delivered', () => {
    expect(deliveryVerdict({ delivery_status: 'delivered', last_delivery_error: null })).toEqual({ delivery: 'delivered' });
    expect(deliveryVerdict({ delivery_status: 'failed', last_delivery_error: 'undelivered-no-pane' })).toEqual({ delivery: 'not-delivered', reason: 'undelivered-no-pane' });
    expect(deliveryVerdict({ delivery_status: 'queued', last_delivery_error: 'target-busy' })).toEqual({ delivery: 'queued', reason: 'target-busy' });
    expect(deliveryVerdict({ delivery_status: 'pending', last_delivery_error: null })).toEqual({ delivery: 'queued' });
    expect(deliveryVerdict(undefined)).toEqual({ delivery: 'not-delivered', reason: 'the message row is gone' });
  });
});
