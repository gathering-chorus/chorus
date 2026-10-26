// @test-type: unit — pure pulse logic on fixtures; no live services
// @domain: messages
// @card: #4756
// @owner: silas
// #4756 — src/announce-boundary: an alert reaches its role's terminal. Since
// #4637 an alert is stored from its owner, so the boundary read it as a
// self-echo and dropped it (row 59465, 10-26 16:44: a pair renudge to
// borg-queen, suppressed). The fixtures are that row and its neighbours.

import { boundaryInput, decide, AnnounceState } from './announce-boundary';

const RENUDGE =
  '[alert for borg-queen · from system | 2026-10-26 16:44 Boston] pair #4753: you have been quiet 120s and kade is waiting on you. Give the next direction, or the pair goes to Abby at 180s.';
const WAKE_BQ =
  '[alert for borg-queen · from system | 2026-10-26 16:50 Boston] wtfu borg-queen: #4753 pair skill-pair-green (run 034mnXtenBpp1a4ioQcyrf) waits on you; you have been idle 4m.';
const WAKE_ABBY =
  '[alert for abby-normal · from system | 2026-10-26 16:50 Boston] wtfu abby-normal: 5 unread nudge(s) waits on you; you have been idle 56m.';
const T0 = Date.parse('2026-10-26T20:44:16Z');

describe('#4756 an alert is judged by its source, not its owner', () => {
  test('the stored renudge (from = to = borg-queen) is delivered', () => {
    const d = decide('borg-queen', 'borg-queen', RENUDGE, T0, new AnnounceState());
    expect(d.deliver).toBe(true);
    expect(d.lane).toBe('machine');
  });

  test('negative proof: a role sending to itself is still a self-echo', () => {
    const d = decide('wren', 'wren', 'note to self: check #4744', T0, new AnnounceState());
    expect(d).toMatchObject({ deliver: false, suppressReason: 'self-echo' });
  });

  test('negative proof: an alert header naming someone else is not taken as an alert', () => {
    // kade quoting borg-queen's alert to wren is kade's message
    expect(boundaryInput('kade', RENUDGE)).toEqual({ from: 'kade', body: RENUDGE });
  });

  test('two alerts to one role are two incidents: the signature is the body, not the header', () => {
    const s = new AnnounceState();
    expect(decide('borg-queen', 'borg-queen', RENUDGE, T0, s).deliver).toBe(true);
    expect(decide('borg-queen', 'borg-queen', WAKE_BQ, T0 + 60_000, s).deliver).toBe(true);
  });

  test('two roles woken in the same tick are both woken', () => {
    const s = new AnnounceState();
    expect(decide('borg-queen', 'borg-queen', WAKE_BQ, T0, s).deliver).toBe(true);
    expect(decide('abby-normal', 'abby-normal', WAKE_ABBY, T0, s).deliver).toBe(true);
  });

  test('negative proof: the same alert again inside the cooldown is still held', () => {
    const s = new AnnounceState();
    expect(decide('borg-queen', 'borg-queen', RENUDGE, T0, s).deliver).toBe(true);
    expect(decide('borg-queen', 'borg-queen', RENUDGE, T0 + 60_000, s)).toMatchObject({ deliver: false, suppressReason: 'dup-class' });
  });
});
