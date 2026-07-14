// @test-type: unit — source-string pins on server.ts wiring, readFileSync of repo files only.
/**
 * #3343 — Jeff's Clearing input rides the pulse delivery worker.
 *
 * The clearing delta is a thin HTTP client (behavior lives pulse-side, tested
 * in platform/pulse: route contract, raw-content storage, kind-aware worker
 * emits, CHECK migration). These tests pin the clearing-side contract in the
 * same source-shape style as socket-ack.test.ts: the retired direct-inject
 * path must be GONE, the supported POST present, and the handler must await
 * delivery so Jeff's ack reflects the hand-off result.
 */
import { readFileSync } from 'fs';
import path from 'path';

const SERVER_SRC = readFileSync(path.join(__dirname, '../src/server.ts'), 'utf-8');

describe('#3343: retired direct chorus-inject path is gone', () => {
  test('no execFileSync of chorus-inject anywhere in the server', () => {
    expect(SERVER_SRC).not.toMatch(/execFileSync\(INJECT_BIN/);
    expect(SERVER_SRC).not.toMatch(/target\/release\/chorus-inject/);
  });
});

describe('#3343: delivery goes through pulse /api/jeff-input', () => {
  test('POSTs to /api/jeff-input with the Clearing caller header', () => {
    expect(SERVER_SRC).toMatch(/\/api\/jeff-input/);
    expect(SERVER_SRC).toMatch(/X-Chorus-Clearing-Caller/);
  });
  test('content travels raw — no [nudge from] framing applied clearing-side', () => {
    const fn = SERVER_SRC.slice(SERVER_SRC.indexOf('async function deliverJeffMessageToTarget'));
    expect(fn.slice(0, 2000)).not.toMatch(/\[nudge from/);
  });
  test('handler routes through processJeffInput — ack means accepted+persisted (#3646)', () => {
    // #3343 pinned ack-after-delivery ("truthful ack"); #3646 redefined the truth:
    // the ack confirms INGEST (persisted), and per-target delivery verdicts travel
    // as 'delivery-status' events. The behavioral contract is pinned in
    // jeff-input-ack.test.ts; this pin holds the wiring.
    expect(SERVER_SRC).toMatch(/processJeffInput\(/);
    expect(SERVER_SRC).toMatch(/deliver: \(target\) => deliverJeffMessageToTarget/);
    expect(SERVER_SRC).toMatch(/socket\.emit\('delivery-status', status\)/);
  });
  test('failure path still emits jeff.input.failed for audit continuity', () => {
    expect(SERVER_SRC).toMatch(/jeff\.input\.failed/);
    expect(SERVER_SRC).toMatch(/jeff\.input\.delivered/);
  });
});
