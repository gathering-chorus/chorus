/**
 * Quarantine of integration-masquerading suites (#2180).
 *
 * The default jest run must exclude suites that boot the full server,
 * hit Fuseki, or assume seeded live state. They remain callable with
 * RUN_INTEGRATION=true (same signal platform/api's coverage script uses).
 *
 * This test locks the list — regressions are caught at the config, not at
 * the next CI failure.
 *
 * Wave 1 (04a996f3): 4 suites. Wave 2: +39, absorbs #2184.
 */

const QUARANTINED = [
  'tests/server-unit\\.test\\.ts$',
  'tests/rca\\.test\\.ts$',
  'tests/completeness-perf\\.test\\.ts$',
  'tests/graph-separation\\.test\\.ts$',
  'tests/observability\\.test\\.ts$',
  'tests/logs-facet\\.test\\.ts$',
  'tests/deploys\\.test\\.ts$',
  'tests/shacl-validation\\.test\\.ts$',
  'tests/crawl-shape\\.test\\.ts$',
  'tests/hooks-summary\\.test\\.ts$',
  'tests/jeff-summary\\.test\\.ts$',
  'tests/quality-summary\\.test\\.ts$',
  'tests/trace-convergence-callstack\\.test\\.ts$',
  'tests/alerts-subdomain\\.test\\.ts$',
  'tests/assessment\\.test\\.ts$',
  'tests/cost-summary\\.test\\.ts$',
  'tests/discover-endpoints\\.test\\.ts$',
  'tests/discover-pages\\.test\\.ts$',
  'tests/domain-dependencies\\.test\\.ts$',
  'tests/fitness-summary\\.test\\.ts$',
  'tests/ollama-resilience\\.test\\.ts$',
  'tests/patterns-summary\\.test\\.ts$',
  'tests/scheduled-reindex\\.test\\.ts$',
  'tests/search-freshness\\.test\\.ts$',
  'tests/session-replay\\.test\\.ts$',
  'tests/trace-envelope\\.test\\.ts$',
  'tests/borg-landing\\.test\\.ts$',
  'tests/chorus-landing\\.test\\.ts$',
  'tests/code-inventory\\.test\\.ts$',
  'tests/crawl-validation\\.test\\.ts$',
  'tests/domain-api-consolidated\\.test\\.ts$',
  'tests/domain-borg-services\\.test\\.ts$',
  'tests/domain-pipeline\\.test\\.ts$',
  'tests/domain-radius\\.test\\.ts$',
  'tests/domain-releases\\.test\\.ts$',
  'tests/domain-section-enrichment\\.test\\.ts$',
  'tests/in-process-harness\\.test\\.ts$',
  'tests/instance-explorer\\.test\\.ts$',
  'tests/spine-event-endpoint\\.test\\.ts$',
  'tests/tests-domain-code\\.test\\.ts$',
  'tests/timestamp\\.test\\.ts$',
  'tests/trace-batch-callstack\\.test\\.ts$',
  'tests/trace-integration-callstack\\.test\\.ts$',
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
