/**
 * Quarantine of integration-masquerading suites (#2180).
 *
 * The default jest run must exclude the 4 suites that boot the full server,
 * hit Fuseki, or assume seeded live state. They remain callable with
 * RUN_INTEGRATION=true (same signal platform/api's coverage script uses).
 *
 * This test locks the list — regressions are caught at the config, not at
 * the next CI failure.
 */

const QUARANTINED = [
  'tests/server-unit\\.test\\.ts$',
  'tests/rca\\.test\\.ts$',
  'tests/completeness-perf\\.test\\.ts$',
  'tests/graph-separation\\.test\\.ts$',
];

function loadConfig(runIntegration: boolean): { testPathIgnorePatterns?: string[] } {
  const original = process.env.RUN_INTEGRATION;
  process.env.RUN_INTEGRATION = runIntegration ? 'true' : '';
  jest.resetModules();
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const config = require('../jest.config.js');
  if (original === undefined) delete process.env.RUN_INTEGRATION;
  else process.env.RUN_INTEGRATION = original;
  return config;
}

describe('jest.config.js — integration quarantine', () => {
  test('default run ignores the 4 integration-masquerading suites', () => {
    const cfg = loadConfig(false);
    const ignored = cfg.testPathIgnorePatterns ?? [];
    for (const pattern of QUARANTINED) {
      expect(
        ignored.some((p) => p.includes(pattern)),
      ).toBe(true);
    }
  });

  test('RUN_INTEGRATION=true lifts the quarantine', () => {
    const cfg = loadConfig(true);
    const ignored = cfg.testPathIgnorePatterns ?? [];
    for (const pattern of QUARANTINED) {
      expect(
        ignored.some((p) => p.includes(pattern)),
      ).toBe(false);
    }
  });

  test('quarantine does not affect node_modules exclusion', () => {
    const cfg = loadConfig(false);
    const ignored = cfg.testPathIgnorePatterns ?? [];
    expect(ignored.some((p) => p.includes('node_modules'))).toBe(true);
  });
});
