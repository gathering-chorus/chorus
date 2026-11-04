// @test-type: unit — signal is fixture-data: pure formatter, no io
// @domain: time — the platform domain this suite guards (#4885)
// @card: #4885
// @owner: silas
/**
 * #4885 — a message's header minute comes from the one formatter (bostonOffsetIso), not from
 * inline toLocaleString copies; the label and the room copy of one message carry one instant.
 */
import { readFileSync } from 'fs';
import { join } from 'path';
import { bostonMinute, bostonOffsetIso } from './boston-iso';

describe('#4885 bostonMinute', () => {
  it('renders Boston wall time to the minute, both sides of DST', () => {
    expect(bostonMinute(new Date('2026-11-04T09:26:30Z'))).toBe('2026-11-04 04:26');
    expect(bostonMinute(new Date('2026-07-01T12:00:00Z'))).toBe('2026-07-01 08:00');
    // the fall-back hour: 05:30Z and 06:30Z are both 01:30 on the wall clock
    expect(bostonMinute(new Date('2026-11-01T05:30:00Z'))).toBe('2026-11-01 01:30');
    expect(bostonMinute(new Date('2026-11-01T06:30:00Z'))).toBe('2026-11-01 01:30');
  });

  it('is the same instant the spine stamp names', () => {
    const d = new Date('2026-11-04T09:26:59.999Z');
    expect(bostonMinute(d)).toBe(bostonOffsetIso(d).slice(0, 16).replace('T', ' '));
  });

  it('NEGATIVE PROOF: no inline Boston formatter is left in the message path', () => {
    for (const f of ['service.ts', 'sender.ts']) {
      const src = readFileSync(join(__dirname, f), 'utf8');
      expect(src).not.toMatch(/toLocaleString\([^)]*America\/New_York/);
      expect(src).not.toMatch(/timeZone:\s*'America\/New_York'/);
    }
    // the check can fail: the pre-#4885 line it guards against is caught
    expect("new Date().toLocaleString('sv-SE', { timeZone: 'America/New_York' })").toMatch(/toLocaleString\([^)]*America\/New_York/);
  });
});
