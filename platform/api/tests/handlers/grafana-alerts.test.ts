// @test-type: unit — stubbed nudge and spine; no Grafana, no pulse.
// @domain: alerts
/**
 * #4666 — Grafana's alerts reach a role (src/handlers/grafana-alerts). Until
 * 10-20 every alert went to receiver `empty`. Each firing alert with a `role`
 * label becomes one nudge to that role with the trigger and values only; an
 * unlabeled one goes nowhere, visibly.
 */
import { alertText, routeGrafanaAlerts, ruleUid } from '../../src/handlers/grafana-alerts';

function sinks() {
  const nudges: Array<[string, string]> = [];
  const rulesSeen: string[] = [];
  const events: Array<[string, Record<string, string>]> = [];
  return {
    nudges, events,
    rules: rulesSeen,
    deps: { nudge: (to: string, text: string, rule: string) => { nudges.push([to, text]); rulesSeen.push(rule); }, spine: (e: string, f: Record<string, string>) => { events.push([e, f]); } },
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
    expect(s.nudges).toEqual([['silas', 'grafana: Sensor run not green — domain sensors, value 1']]);
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

  // #4717 (Jeff 10-24 06:03): the rule, the threshold and the values in words, never refIds
  test('the text says the series and the query value, or the rule summary, never a refId', () => {
    expect(alertText({ labels: { alertname: 'Disk', mount: '/' }, values: { A: 97, C: 1 } })).toBe('grafana: Disk — mount /, value 97');
    expect(alertText({ labels: { alertname: 'Disk' }, valueString: "[ var='A' labels={} value=97 ], [ var='C' labels={} value=1 ]" })).toBe('grafana: Disk — value 97');
    expect(alertText({ labels: { alertname: 'SHACL violations rose' }, annotations: { summary: 'violations 812, yesterday 790 (alerts when above yesterday)' }, values: { A: 812, C: 1 } }))
      .toBe('grafana: SHACL violations rose — violations 812, yesterday 790 (alerts when above yesterday)');
  });

  // NEGATIVE PROOF: the shapes of tonight's payloads (A=1 B=1 C=1, A=2 C=1) carry no refId into the text
  test('no refId reaches the nudge, whatever shape the payload has', () => {
    for (const a of [
      { labels: { alertname: 'Sensor run not green', role: 'silas', sensor: 'services', owner: 'silas' }, values: { A: 1, B: 1, C: 1 } },
      { labels: { alertname: 'Messages lost in a day', role: 'wren' }, values: { A: 2, C: 1 } },
      { labels: { alertname: 'x' }, valueString: "[ var='A' labels={} value=2 ], [ var='C' labels={} value=1 ]" },
    ]) {
      expect(alertText(a)).not.toMatch(/\b[A-Z]=/);
    }
  });

  test('an empty payload routes nothing', () => {
    const s = sinks();
    expect(routeGrafanaAlerts({}, s.deps)).toEqual({ routed: [], unrouted: [] });
    expect(s.nudges).toEqual([]);
  });

  // #4970 — the rule uid rides to ops-nudge, which reads that Alert row's notify.
  test('the rule uid from generatorURL is handed to the nudge, so ops-nudge can read its row', () => {
    const s = sinks();
    routeGrafanaAlerts({ alerts: [{ ...sensorRed, generatorURL: 'http://localhost:3000/alerting/grafana/sensor-run-not-green/view?orgId=1' }] }, s.deps);
    expect(s.rules).toEqual(['sensor-run-not-green']);
  });

  // NEGATIVE PROOF: no URL, or one that is not a rule view, names no rule (nothing gated).
  test('no generatorURL, or a foreign one, names no rule', () => {
    expect(ruleUid({})).toBe('');
    expect(ruleUid({ generatorURL: 'http://x/alerting/list' })).toBe('');
    expect(ruleUid({ generatorURL: 'http://x/alerting/grafana/a b/view' })).toBe('');
  });
});
