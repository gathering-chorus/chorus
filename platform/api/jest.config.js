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
  // Coverage: #2161 floor.
  coverageThreshold: {
    global: {
      branches: 60,
      functions: 75,
      lines: 80,
      statements: 80,
    },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
};
