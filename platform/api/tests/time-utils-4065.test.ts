// @test-type: unit
// #4065 — the spine has ONE clock (#3880 / ADR-058): offset-ISO on the Boston
// clock. bostonNow() is a naive display label; the spine-event route wrote it
// to ~/.chorus/chorus.log and every seed.received line failed the
// spine-one-clock health check. These proofs pin the writer to the format the
// check accepts, and show the check would still reject the old label.
import { bostonNow, bostonOffsetIso } from '../src/time-utils';

// The exact acceptance rule from chorus-health spine-one-clock: an offset
// suffix; "Z" and naive strings are the two failure classes it names.
const OFFSET_ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[+-]\d{2}:?\d{2}$/;
const oneClockAccepts = (ts: string): boolean => !ts.endsWith('Z') && ts.match(/[+-]\d{2}:?\d{2}$/) !== null;

test('#4065 bostonOffsetIso is offset-ISO with millis and the Boston offset', () => {
  const ts = bostonOffsetIso(new Date('2026-09-02T15:08:15.123Z'));
  expect(ts).toMatch(OFFSET_ISO);
  expect(ts).toBe('2026-09-02T11:08:15.123-0400');
  expect(oneClockAccepts(ts)).toBe(true);
});

test('#4065 winter dates carry -0500', () => {
  const ts = bostonOffsetIso(new Date('2026-01-15T15:08:15.000Z'));
  expect(ts).toBe('2026-01-15T10:08:15.000-0500');
});

test('#4065 NEGATIVE PROOF: the old label (bostonNow) is exactly what the check rejects', () => {
  const naive = bostonNow();
  expect(naive).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  expect(oneClockAccepts(naive)).toBe(false);
  expect(oneClockAccepts('2026-09-02T15:08:15.123Z')).toBe(false);
});
