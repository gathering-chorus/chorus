// @test-type: unit — pure formatter, no io
// @domain: time
// @card: #4885
// @owner: silas
import { bostonOffset, isBostonDaylight, bostonOffsetIso, bostonLocal, bostonMinute } from '../time/boston';

describe('#4885 the one Boston renderer', () => {
  it('takes DST from the tz database on both transition days', () => {
    // 2026-03-08: 06:59Z is 01:59 EST, 07:00Z is 03:00 EDT
    expect(bostonOffsetIso(new Date('2026-03-08T06:59:00Z'))).toBe('2026-03-08T01:59:00.000-05:00');
    expect(bostonOffsetIso(new Date('2026-03-08T07:00:00Z'))).toBe('2026-03-08T03:00:00.000-04:00');
    // 2026-11-01: 05:30Z is 01:30 EDT, 06:30Z is 01:30 EST — same wall minute, two instants
    expect(bostonOffsetIso(new Date('2026-11-01T05:30:00Z'))).toBe('2026-11-01T01:30:00.000-04:00');
    expect(bostonOffsetIso(new Date('2026-11-01T06:30:00Z'))).toBe('2026-11-01T01:30:00.000-05:00');
  });

  it('NEGATIVE PROOF: the old month rule is wrong where the tz database is right', () => {
    // the api's isEDT called 2026-11-01 00:30 EDT "EST" (it switched at the start of the day)
    const d = new Date('2026-11-01T04:30:00Z');
    // the retired rule, verbatim in effect: November is EDT only before the first Sunday's date
    const monthRule = (x: Date) => { const nov1 = new Date(Date.UTC(x.getUTCFullYear(), 10, 1)).getUTCDay();
      const firstSunday = nov1 === 0 ? 1 : 8 - nov1; return x.getUTCDate() < firstSunday; };
    expect(monthRule(d)).toBe(false);            // the old answer: EST
    expect(isBostonDaylight(d)).toBe(true);      // the tz database: still EDT until 02:00
    expect(bostonOffset(d)).toBe('-04:00');
  });

  it('renders the label forms from the same instant', () => {
    const d = new Date('2026-11-04T09:26:30.123Z');
    expect(bostonLocal(d)).toBe('2026-11-04 04:26:30');
    expect(bostonMinute(d)).toBe('2026-11-04 04:26');
    expect(bostonOffsetIso(d)).toBe('2026-11-04T04:26:30.123-05:00');
  });
});
