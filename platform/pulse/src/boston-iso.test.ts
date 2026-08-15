// @test-type: unit — signal is fixture-data: pure formatter, no io
/**
 * #3880 — one clock on the spine. bostonOffsetIso() must emit offset-ISO in
 * America/New_York, the format chorus-events (the majority writer) uses, so
 * every reader sorts/joins on one clock. The daily "timestamp issue" was
 * pulse writing UTC-Z next to chorus-events' -04:00.
 */
import { bostonOffsetIso } from './boston-iso';

describe('#3880 bostonOffsetIso', () => {
  it('emits offset-ISO (never Z, never naive)', () => {
    const s = bostonOffsetIso(new Date('2026-08-14T21:00:00.000Z'));
    expect(s).toMatch(/[+-]\d{2}:\d{2}$/);
    expect(s.endsWith('Z')).toBe(false);
  });

  it('summer date renders EDT (-04:00) wall clock', () => {
    expect(bostonOffsetIso(new Date('2026-08-14T21:00:00.000Z'))).toBe('2026-08-14T17:00:00.000-04:00');
  });

  it('winter date renders EST (-05:00) wall clock — DST is computed, not hardcoded', () => {
    expect(bostonOffsetIso(new Date('2026-01-14T21:00:00.000Z'))).toBe('2026-01-14T16:00:00.000-05:00');
  });

  // NEGATIVE PROOF (#3734): the parsed instant must equal the input instant —
  // a formatter that shifts wall time without fixing the offset corrupts order.
  it('round-trips to the same instant', () => {
    const d = new Date('2026-08-14T21:07:33.123Z');
    expect(Date.parse(bostonOffsetIso(d))).toBe(d.getTime());
  });
});
