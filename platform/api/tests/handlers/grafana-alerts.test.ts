// @test-type: unit — stubbed nudge and spine; no Grafana, no pulse.
// @domain: alerts
/**
 * #4666 — Grafana's alerts reach a role (src/handlers/grafana-alerts). Until
 * 10-20 every alert went to receiver `empty`. Each firing alert with a `role`
 * label becomes one nudge to that role with the trigger and values only; an
 * unlabeled one goes nowhere, visibly.
 */
import { alertText, routeGrafanaAlerts } from '../../src/handlers/grafana-alerts';

function sinks() {
  const nudges: Array<[string, string]> = [];
  const events: Array<[string, Record<string, string>]> = [];
  return {
    nudges, events,
    deps: { nudge: (to: string, text: string) => { nudges.push([to, text]); }, spine: (e: string, f: Record<string, string>) => { events.push([e, f]); } },
  };
}

const sensorRed = {
  status: 'firing',
  labels: { alertname: 'Sensor run not green', role: 'silas', domain: 'sensors' },
  values: { A: 1 },
};

describe('#4666 grafana alerts → role nudges', () => {
  test('a firing alert with a role label nudges that role with the trigger and values', () => {
    const s = sinks();
    const r = routeGrafanaAlerts({ alerts: [sensorRed] }, s.deps);
    expect(s.nudges).toEqual([['silas', 'grafana: Sensor run not green — A=1']]);
    expect(r.routed).toHaveLength(1);
  });

  // NEGATIVE PROOF: an unlabeled alert (today's chorus-no-sessions-48h) must not page anyone.
  test('a firing alert with no role label goes nowhere, and is recorded as unrouted', () => {
    const s = sinks();
    const r = routeGrafanaAlerts({ alerts: [{ status: 'firing', labels: { alertname: 'No team sessions in 48 hours' } }] }, s.deps);
    expect(s.nudges).toEqual([]);
    expect(r.unrouted).toEqual(['No team sessions in 48 hours']);
    expect(s.events[0][0]).toBe('alert.unrouted');
  });

  test('a role label that is not a role is unrouted, never sent', () => {
    const s = sinks();
    const r = routeGrafanaAlerts({ alerts: [{ status: 'firing', labels: { alertname: 'x', role: 'everyone' } }] }, s.deps);
    expect(s.nudges).toEqual([]);
    expect(r.unrouted).toEqual(['x']);
  });

  test('a resolved alert pages no one', () => {
    const s = sinks();
    const r = routeGrafanaAlerts({ alerts: [{ ...sensorRed, status: 'resolved' }] }, s.deps);
    expect(s.nudges).toEqual([]);
    expect(r.routed).toEqual([]);
  });

  test('the text carries no annotation prose, only the name and values', () => {
    expect(alertText({ labels: { alertname: 'Disk' }, values: { A: 97 }, valueString: '[ var=A value=97 ]' })).toBe('grafana: Disk — A=97');
    expect(alertText({ labels: { alertname: 'Disk' }, valueString: '[ var=A value=97 ]' })).toBe('grafana: Disk — [ var=A value=97 ]');
  });

  test('an empty payload routes nothing', () => {
    const s = sinks();
    expect(routeGrafanaAlerts({}, s.deps)).toEqual({ routed: [], unrouted: [] });
    expect(s.nudges).toEqual([]);
  });
});
