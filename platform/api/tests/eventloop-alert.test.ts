// #3050 — the eventloop alert must report ONLY what it measured (duration + time),
// and must NOT fabricate a causal story. This test is the honest-message contract:
// the day's whole lesson — an alert that asserts the unobservable can't be trusted.
// #3079 — added op param; op=unknown → access-log pointer; op=name → captured-op note.
import { formatBlockAlert } from '../src/eventloop-alert';

const TS = '2026-05-23T20:00:00.000Z';

describe('#3050 formatBlockAlert — honest, no fabrication', () => {
  it('carries the real measured duration', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    expect(a.duration_ms).toBe(1234);
    expect(a.message).toContain('1234');
  });

  it('carries the timestamp so the slow request is correlatable in the access log (op=unknown)', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    expect(a.ts).toBe(TS);
    expect(a.message).toContain(TS);
    expect(a.message.toLowerCase()).toContain('access log');
  });

  it('reports captured op when op is known', () => {
    const a = formatBlockAlert(1234, TS, 'scheduledReindex');
    expect(a.op).toBe('scheduledReindex');
    expect(a.message.toLowerCase()).toContain('captured op: scheduledreindex');
    expect(a.message.toLowerCase()).not.toContain('access log');
  });

  it('does NOT fabricate a causal story it cannot observe', () => {
    const a = formatBlockAlert(1234, TS, 'unknown');
    const m = a.message.toLowerCase();
    expect(m).not.toContain('blocked every role');
    expect(m).not.toContain('stalled');
    expect(m).not.toContain('sync git');
    expect(m).not.toContain('#3039 freeze class');
  });
});
