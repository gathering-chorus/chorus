module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  // Integration tests hit the live chorus-api on :3340 — parallel workers
  // contend for the server and cause spurious timeouts/404 flakes. Single
  // worker keeps the suite deterministic at the cost of total runtime.
  maxWorkers: 1,
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
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
};
