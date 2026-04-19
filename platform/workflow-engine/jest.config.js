module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['**/tests/**/*.test.ts'],
  // Coverage floor (#2161).
  coverageThreshold: {
    global: { branches: 60, functions: 75, lines: 80, statements: 80 },
  },
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.d.ts',
    '!src/**/*.test.ts',
    '!src/cli.ts',
    '!src/index.ts',
  ],
  // Quiet reporter by default (#2225). JEST_VERBOSE=true for full output.
  reporters: process.env.JEST_VERBOSE === 'true'
    ? ['default']
    : [['summary', { summaryThreshold: 0 }]],
};
