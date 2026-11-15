// @test-type: unit — builds the disk Alert row with the real generator, fills its
// summary the way Grafana fills {{ $labels.x }}, and reads the text alertText makes.
// @domain: alerts
/**
 * #4977 AC1 — the disk alert names the volume that is over the line. On 2026-11-14
 * 20:32 the three #4970 alerts arrived with one text, naming no volume: the row's
 * summary said only the question, and alertText prints the summary alone. The
 * test reads the text a person receives, from the labels Grafana sent that night.
 */
import { execFileSync } from 'child_process';
import * as path from 'path';
import { alertText } from '../../src/handlers/grafana-alerts';

const ROOT = path.resolve(__dirname, '../../../..');
const MON = path.join(ROOT, 'designing/domains/monitors');

function built(monitor = path.join(MON, 'bedroom-disk-full-monitor-4970.json')): { comment: string; label: string } {
  const out = execFileSync('python3', [path.join(ROOT, 'platform/scripts/monitor-alert'), 'build', monitor, path.join(MON, 'infra-disk-sensor-4970.json')]);
  return JSON.parse(out.toString());
}

/** Grafana fills {{ $labels.name }} from the alert's labels. Only that form is used in the row. */
function fill(template: string, labels: Record<string, string>): string {
  return template.replace(/\{\{\s*\$labels\.([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g, (_m, k) => labels[k] ?? `<no value>`);
}

// the labels of one of the three alerts Grafana sent at 20:32
const firing = {
  alertname: 'A Bedroom (or Library) disk is more than 99% full', role: 'silas', grafana_folder: 'Chorus',
  instance: '192.168.86.242:9100', mountpoint: '/Volumes/VideosKem-Lat', fstype: 'apfs', device: '/dev/disk30s1',
};

describe('#4977 AC1 the disk alert names its volume', () => {
  test('the text a person receives names the volume and the machine', () => {
    const row = built();
    const text = alertText({ status: 'firing', labels: firing, annotations: { summary: fill(row.comment, firing) } });
    expect(text).toContain('/Volumes/VideosKem-Lat on 192.168.86.242:9100 is more than 99% full');
    expect(text).not.toContain('{{');
    expect(text).not.toContain('<no value>');
  });

  test('three volumes over the line read as three different texts', () => {
    const row = built();
    const texts = ['VideosKem-Lat', 'VideosRi-Tay', 'VideosTay-Vin'].map((v) =>
      alertText({ status: 'firing', labels: { ...firing, mountpoint: `/Volumes/${v}` }, annotations: { summary: fill(row.comment, { ...firing, mountpoint: `/Volumes/${v}` }) } }));
    expect(new Set(texts).size).toBe(3);
  });

  // NEGATIVE PROOF: the #4970 row (no monitorSays) produced a text with no volume, as on 11-14 20:32.
  test('a monitor that says nothing of the series gives a text with no volume name, and this test can tell', () => {
    const fs = require('fs');
    const os = require('os');
    const m = JSON.parse(fs.readFileSync(path.join(MON, 'bedroom-disk-full-monitor-4970.json'), 'utf8'));
    delete m.data.monitorSays;
    const tmp = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'm4977-')), 'm.json');
    fs.writeFileSync(tmp, JSON.stringify(m));
    const row = built(tmp);
    const text = alertText({ status: 'firing', labels: firing, annotations: { summary: fill(row.comment, firing) } });
    expect(text).not.toContain('VideosKem-Lat');
  });
});
