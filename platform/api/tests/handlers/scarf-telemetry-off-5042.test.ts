// @test-type: unit — reads platform/api/package.json; no network
// @domain: security
// @card: #5042
/**
 * #5042 (Silas, 2026-11-27 21:37): swagger-ui-express brings swagger-ui-dist, which depends on
 * @scarf/scarf, a telemetry package with an install script that reports installs to a third
 * party. chorus-api turns it off in its own package.json (scarfSettings.enabled: false), the
 * setting @scarf/scarf reads from the installing package.
 * NEGATIVE: a package.json without the setting, or with it on, is refused by the same check.
 */
import * as fs from 'fs';
import * as path from 'path';

/** Why a package.json leaves scarf telemetry on, or null when it is off. */
function scarfLeftOn(pkg: { scarfSettings?: { enabled?: unknown } }): string | null {
  if (!pkg.scarfSettings) return 'no scarfSettings: @scarf/scarf reports installs by default';
  return pkg.scarfSettings.enabled === false ? null : `scarfSettings.enabled is ${JSON.stringify(pkg.scarfSettings.enabled)}, not false`;
}

const PKG = JSON.parse(fs.readFileSync(path.join(__dirname, '..', '..', 'package.json'), 'utf-8'));

test('chorus-api turns scarf telemetry off', () => {
  expect(scarfLeftOn(PKG)).toBeNull();
});

test('NEGATIVE: a package.json without the setting, or with it on, is named', () => {
  const { scarfSettings: _off, ...without } = PKG;
  expect(scarfLeftOn(without)).toMatch(/no scarfSettings/);
  expect(scarfLeftOn({ ...PKG, scarfSettings: { enabled: true } })).toMatch(/not false/);
});
