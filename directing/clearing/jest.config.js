module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  moduleFileExtensions: ['ts', 'js', 'json'],
  // Coverage floor — #2167 calibrated to baseline, not aspiration.
  //
  // Baseline 2026-04-17: 10% stmts / 14.65% branches / 6.45% fns / 9.9% lines.
  //
  // Why so low? The Clearing test suite is predominantly HTTP integration
  // against a subprocess Clearing (see beforeAll spawn in clearing-ui.test.ts
  // from #2166). Tests exercise behavior end-to-end — router classify, filter
  // logic, Socket.IO fanout, persistence — but jest only instruments code
  // loaded in the test process. The subprocess server.ts / router.ts code
  // runs outside instrumentation, so line-level coverage misses most of src/.
  //
  // Raising the floor requires either:
  //   - In-process server factory (import server into tests, no subprocess)
  //   - Coverage proxy on the subprocess (nyc instrument + collect-coverage)
  // Both are separate scope from coverage-tooling wiring (#2167).
  //
  // Gate is calibrated just below baseline so regressions in the tested
  // surface (unit tests: router, tiles, participants, session-tailer) trip
  // the build without demanding the architectural refactor.
  coverageThreshold: {
    global: { branches: 12, functions: 5, lines: 8, statements: 8 },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
};
