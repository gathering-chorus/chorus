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
  // ts-jest diagnostics off — type checking is tsc's job, not the test runner's.
  // Tests in this dir were written for default-jest (no strict TS) and use
  // `body.data` style access on `unknown`-typed `res.json()` returns.
  transform: {
    '^.+\\.tsx?$': ['ts-jest', { diagnostics: false }],
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
};
