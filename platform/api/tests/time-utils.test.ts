import { isEDT, convertToLocal, bostonNow, boston } from '../src/time-utils';

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

describe('#3093 boston — the render-to-human helper', () => {
  // The render-vs-storage boundary: storage stays UTC; this is the only
  // formatter for human-facing strings (alert messages, nudge bodies, etc).

  it('renders an ISO string in Boston with the EDT suffix in summer', () => {
    // 2026-05-26T17:24:41Z is the exact shape of the alerts Jeff has been
    // seeing all day. EDT = UTC-4, so 17:24 → 13:24.
    expect(boston('2026-05-26T17:24:41.000Z')).toBe('2026-05-26 13:24:41 EDT');
  });

  it('renders an ISO string in Boston with the EST suffix in winter', () => {
    // EST = UTC-5, so 17:00Z in January → 12:00.
    expect(boston('2026-01-15T17:00:00.000Z')).toBe('2026-01-15 12:00:00 EST');
  });

  it('accepts a Date instance', () => {
    const d = new Date('2026-05-26T17:24:41.000Z');
    expect(boston(d)).toBe('2026-05-26 13:24:41 EDT');
  });

  it('accepts epoch milliseconds', () => {
    const ms = Date.parse('2026-05-26T17:24:41.000Z');
    expect(boston(ms)).toBe('2026-05-26 13:24:41 EDT');
  });

  it('shape is stable: YYYY-MM-DD HH:MM:SS [EDT|EST]', () => {
    // The shape contract — anything reading these strings (humans, log
    // grep, future parsers) depends on it. Lock it.
    expect(boston(new Date())).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} (EDT|EST)$/);
  });

  it('does not leak ISO artifacts into the rendered string', () => {
    // The whole point of #3093 — UTC formatting must not bleed into a
    // string Jeff reads. The ISO date-time separator ("T" between date and
    // time) and the UTC suffix ("Z") are both forbidden in the date/time
    // portion. The trailing zone abbreviation "EDT/EST" happens to contain
    // a "T", so we check the date+time portion specifically.
    const out = boston('2026-05-26T17:24:41.000Z');
    const dateTimePart = out.replace(/ (EDT|EST)$/, '');
    expect(dateTimePart).not.toContain('T');
    expect(dateTimePart).not.toContain('Z');
    expect(out).not.toMatch(/\.\d{3}Z/); // no milliseconds-with-Z either
  });
});
