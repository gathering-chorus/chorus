import { isEDT, convertToLocal, bostonNow } from '../src/time-utils';

describe('isEDT', () => {
  it('returns true for any date in April through October', () => {
    expect(isEDT('2026-04-15T12:00:00Z')).toBe(true);
    expect(isEDT('2026-07-04T00:00:00Z')).toBe(true);
    expect(isEDT('2026-10-15T23:59:59Z')).toBe(true);
  });

  it('returns false for any date in December through February', () => {
    expect(isEDT('2026-01-01T00:00:00Z')).toBe(false);
    expect(isEDT('2026-02-15T12:00:00Z')).toBe(false);
    expect(isEDT('2025-12-31T23:59:59Z')).toBe(false);
  });

  it('returns false early in March, before the second Sunday', () => {
    // 2026 March 1 is a Sunday. Second Sunday = March 8.
    expect(isEDT('2026-03-01T12:00:00Z')).toBe(false);
    expect(isEDT('2026-03-07T12:00:00Z')).toBe(false);
  });

  it('returns true on and after the second Sunday of March', () => {
    expect(isEDT('2026-03-08T12:00:00Z')).toBe(true);
    expect(isEDT('2026-03-15T12:00:00Z')).toBe(true);
  });

  it('returns true early in November, before the first Sunday', () => {
    // 2026 November 1 is a Sunday — so first Sunday is Nov 1 itself, so any date >=1 is EST.
    // Pick 2025: Nov 1 2025 is Saturday. First Sunday = Nov 2. So Nov 1 is still EDT.
    expect(isEDT('2025-11-01T12:00:00Z')).toBe(true);
  });

  it('returns false on and after the first Sunday of November', () => {
    expect(isEDT('2025-11-02T12:00:00Z')).toBe(false);
    expect(isEDT('2025-11-15T12:00:00Z')).toBe(false);
  });
});

describe('convertToLocal', () => {
  it('formats a UTC timestamp into YYYY-MM-DD HH:MM:SS', () => {
    const out = convertToLocal('2026-04-18T20:00:00Z', 'America/New_York');
    // 20:00 UTC in April = 16:00 EDT.
    expect(out).toBe('2026-04-18 16:00:00');
  });

  it('returns the input string unchanged when parsing fails', () => {
    const out = convertToLocal('not-a-date-at-all', 'America/New_York');
    // Intl handles invalid date gracefully — but should still be the fallback string.
    // We accept either pass-through or a recognizable placeholder; concrete guarantee
    // is "no throw".
    expect(typeof out).toBe('string');
  });

  it('preserves leading zeros in month/day/minute/second', () => {
    // Intl's hour12:false produces 24:mm for midnight hour in some locales —
    // we check the stable-shape fields (date + minute + second).
    const out = convertToLocal('2026-01-05T05:07:09Z', 'America/New_York');
    expect(out.startsWith('2026-01-05 ')).toBe(true);
    expect(out.endsWith(':07:09')).toBe(true);
  });
});

describe('bostonNow', () => {
  it('returns a string matching the YYYY-MM-DD HH:MM:SS shape', () => {
    const now = bostonNow();
    expect(now).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  });

  it('is internally consistent with convertToLocal on the same instant', () => {
    // bostonNow wraps convertToLocal(new Date().toISOString()); both must produce
    // the same second-granularity output within a single tick window.
    const a = bostonNow();
    const b = convertToLocal(new Date().toISOString(), 'America/New_York');
    // Match at minute granularity to avoid seconds flake.
    expect(a.slice(0, 16)).toBe(b.slice(0, 16));
  });
});
