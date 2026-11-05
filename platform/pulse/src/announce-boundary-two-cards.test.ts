// @test-type: unit — pure pulse logic; no live services
// @domain: messages
// @card: #4889
// @owner: silas
// #4889 (folded #4891) — src/announce-boundary: two cards failing at the same step
// inside the cooldown each reach their own owner. The signature is the first 48 chars
// with digits as N, so "#4889 cicd failed at test-pipeline" and "#4890 cicd failed at
// test-pipeline" are one signature, and the window is recipient-agnostic (Jeff's 20×3).

import { decide, AnnounceState } from './announce-boundary';

const COOLDOWN = 30 * 60_000;
const t0 = Date.parse('2026-11-05T06:16:00-05:00');
const failed = (role: string, card: number) =>
  `[alert for ${role} · from system | 2026-11-05 06:16 Boston] #${card} cicd failed · /cws ${card}\n\`\`\`\ncicd failed at test-pipeline: werk-test: RED (exit 1)\n\`\`\``;

describe('#4889 two cards, one step, one cooldown', () => {
  it('each card reaches its own owner', () => {
    const st = new AnnounceState(COOLDOWN);
    expect(decide('silas', 'silas', failed('silas', 4889), t0, st, 'run-finished').deliver).toBe(true);
    expect(decide('kade', 'kade', failed('kade', 4890), t0 + 5 * 60_000, st, 'run-finished').deliver).toBe(true);
  });

  it('NEGATIVE PROOF: without the wake mark the second card is swallowed by the first one\'s window', () => {
    const st = new AnnounceState(COOLDOWN);
    expect(decide('silas', 'silas', failed('silas', 4889), t0, st).deliver).toBe(true);
    expect(decide('kade', 'kade', failed('kade', 4890), t0 + 5 * 60_000, st)).toMatchObject({ deliver: false, suppressReason: 'dup-class' });
  });
});
