module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  // maxWorkers history:
  //   6734fe2d (#2161): pinned to 1 — parallel workers hit shared :3340, caused
  //     flakes. 50 tests moved from fail to pass under serial.
  //   #2173 AC4 (this session): tests moved off live :3340 onto the in-process
  //     harness (tests/lib/test-app.ts). Each worker gets its own ephemeral
  //     port from app.listen(0). Empirical: 101s serial → 38s at 8 workers
  //     across 393 passing tests. Lifted to 50% as a conservative step
  //     because a handful of tests still hit real Fuseki/SQLite with
  //     state-mutating writes; those get mocked at handler seams as
  //     decomposition lands. If flakes return, lower, don't re-pin to 1.
  maxWorkers: '50%',
  // #2272: quarantine eliminated. Suites converted to in-process harness or deleted.
  // athena.test.ts is the one remaining excluded suite — genuine integration test (live Fuseki
  // + Chorus API on :3340). Run: RUN_INTEGRATION=true npx jest tests/athena.test.ts
  //
  // #2271: WHY this test doesn't contribute to coverage even with RUN_INTEGRATION=true —
  // athena.test.ts makes HTTP calls to an external chorus-api process. Jest instruments code
  // loaded in its own process; src/ handlers executing those requests run in a separate Node
  // process (LaunchAgent) and are never touched by Jest's instrumenter. This is a subprocess
  // boundary: no amount of RUN_INTEGRATION=true flags fixes it. To get real coverage from
  // athena.test.ts, convert it to use the in-process startTestApp harness (same pattern as
  // server-unit.test.ts). Tracked as follow-on to #2271.
  testPathIgnorePatterns: process.env.RUN_INTEGRATION === 'true' ? ['/node_modules/'] : [
    '/node_modules/',
    '<rootDir>/tests/athena\\.test\\.ts$',
    // #2495 / #2501 — integration tests masquerading as unit. They hit
    // /api/athena/subdomains/.../completeness (or sibling endpoints) on the
    // in-process harness, which proxies to live Fuseki. No Fuseki in CI for
    // the jest job → 500s. Quarantined behind RUN_INTEGRATION until each is
    // converted to a mocked Fuseki seam. Run locally with RUN_INTEGRATION=true.
    '<rootDir>/tests/observability\\.test\\.ts$',
    '<rootDir>/tests/domain-api-consolidated\\.test\\.ts$',
    '<rootDir>/tests/trace-integration-callstack\\.test\\.ts$',
    '<rootDir>/tests/trace-batch-callstack\\.test\\.ts$',
    '<rootDir>/tests/trace-convergence-callstack\\.test\\.ts$',
    '<rootDir>/tests/logs-facet\\.test\\.ts$',
    '<rootDir>/tests/quality-summary\\.test\\.ts$',
    '<rootDir>/tests/regression/athena-subdomain-batch\\.regression\\.test\\.ts$',
    '<rootDir>/tests/domain-pipeline\\.test\\.ts$',
    '<rootDir>/tests/handlers/sessions\\.test\\.ts$',
    '<rootDir>/tests/discover-pages\\.test\\.ts$',
    '<rootDir>/tests/scheduled-reindex\\.test\\.ts$',
    '<rootDir>/tests/discover-endpoints\\.test\\.ts$',
    '<rootDir>/tests/spine-event-endpoint\\.test\\.ts$',
    '<rootDir>/tests/deploys\\.test\\.ts$',
    '<rootDir>/tests/domain-releases\\.test\\.ts$',
    '<rootDir>/tests/shacl-validation\\.test\\.ts$',
    '<rootDir>/tests/assessment\\.test\\.ts$',
    '<rootDir>/tests/search-freshness\\.test\\.ts$',
    '<rootDir>/tests/domain-borg-services\\.test\\.ts$',
    '<rootDir>/tests/ollama-resilience\\.test\\.ts$',
    '<rootDir>/tests/tests-domain-code\\.test\\.ts$',
    '<rootDir>/tests/alerts-subdomain\\.test\\.ts$',
    '<rootDir>/tests/timestamp\\.test\\.ts$',
    '<rootDir>/tests/code-inventory\\.test\\.ts$',
  ],
  // ts-jest diagnostics off — type checking is tsc's job, not the test runner's.
  // Tests in this dir were written for default-jest (no strict TS) and use
  // `body.data` style access on `unknown`-typed `res.json()` returns.
  // #2495: tsconfig.json moves to module=node16 so tsc honors the SDK's
  // exports field. ts-jest stays on commonjs so dynamic `await import()`
  // calls in tests (e.g., test-app.ts) get transpiled to require(),
  // avoiding the --experimental-vm-modules requirement. Runtime Node
  // honors the SDK exports field regardless of TS module setting.
  transform: {
    '^.+\\.tsx?$': ['ts-jest', {
      diagnostics: false,
      tsconfig: { module: 'commonjs', moduleResolution: 'node', esModuleInterop: true },
    }],
  },
  // Coverage floor — #2167 calibrated to baseline.
  //
  // Baseline 2026-04-17: 0% across all metrics. Every test in this suite is
  // an HTTP integration test against the live chorus-api on :3340, which
  // runs as a separate Node process (LaunchAgent). Jest only instruments
  // code loaded in the test process, so the server.ts / handlers exercised
  // by those HTTP calls aren't counted.
  //
  // Floor kept at 0 to not block the pipeline. Real coverage requires either:
  //   - Refactor: import handlers into tests and call them directly
  //   - Subprocess instrumentation: nyc or c8 wrapping the chorus-api service
  // Both are separate scope from #2167 (tooling-wiring). This config gets
  // the measurement pipe in place so future lifts are visible.
  coverageThreshold: {
    global: {
      branches: 0,
      functions: 0,
      lines: 0,
      statements: 0,
    },
    'src/patterns-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/session-replay.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/jeff-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/hooks-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/fitness-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/cost-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    'src/quality-summary.ts': {
      branches: 60, functions: 75, lines: 80, statements: 80,
    },
    // server.ts — 7225 lines, 136 route handlers, lifted from 0% to ~9% by
    // the in-process smoke suite (#2167). Reaching 60/75/80 requires
    // converting the ~40 existing HTTP integration tests in tests/ to use
    // the imported `app` via the require.main guard + hitting a larger
    // share of the 136 routes with downstream mocks. Per-file threshold is
    // set at the current floor so regressions trip the build; raising
    // it is its own multi-file conversion effort.
    'src/server.ts': {
      branches: 1, functions: 1, lines: 5, statements: 5,
    },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
  // Quiet reporter by default (#2225). Cuts TTY progress chatter; failures
  // still surface via summary with summaryThreshold: 0. Set JEST_VERBOSE=true
  // to fall back to the default reporter for debugging.
  reporters: process.env.JEST_VERBOSE === 'true'
    ? ['default']
    : [['summary', { summaryThreshold: 0 }]],
};
