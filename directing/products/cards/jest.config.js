module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  // #2504 (a-vikunja): tests that hit live Vikunja API require VIKUNJA_TOKEN
  // and a reachable Vikunja instance. CI has neither. Same RUN_INTEGRATION
  // gate pattern as platform/api/jest.config.js (#2501 precedent — 20 tests
  // there). Run locally with RUN_INTEGRATION=true to exercise these.
  testPathIgnorePatterns: process.env.RUN_INTEGRATION === 'true' ? ['/node_modules/'] : [
    '/node_modules/',
    '<rootDir>/tests/origin-labels\\.test\\.ts$',
    '<rootDir>/tests/sequence-labels\\.test\\.ts$',
  ],
  // Coverage floor (#2161).
  coverageThreshold: {
    global: { branches: 60, functions: 75, lines: 80, statements: 80 },
  },
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts', '!src/**/*.test.ts'],
  // Quiet reporter by default (#2225). JEST_VERBOSE=true for full output.
  reporters: process.env.JEST_VERBOSE === 'true'
    ? ['default']
    : [['summary', { summaryThreshold: 0 }]],
};
