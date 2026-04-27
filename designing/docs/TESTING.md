# Testing Documentation

**Last updated**: 2026-02-21 by Kade (Engineer)

This document provides comprehensive information about the testing approach for the Jeff Bridwell Personal Site application.

## Testing Philosophy

1. **Real Implementation Testing**: Tests use the real implementation code rather than mocks whenever possible to ensure accurate code coverage metrics.
2. **External Dependency Mocking**: Only external dependencies (file system, network calls) should be mocked.
3. **High Coverage Standards**: Maintain >80% code coverage for all core components.
4. **Test All Layers**: Unit, integration, security, and performance tests are all essential.

## Current Coverage Status

As of 2026-02-10, the project has **83% statement coverage** with an **80% enforcement threshold** in `jest.config.js`.

```javascript
// jest.config.js
coverageThreshold: {
  global: {
    statements: 80,
    branches: 60,
    lines: 80,
    functions: 75
  },
  './src/auth/solid-auth.ts': {
    statements: 80,
    branches: 60,
    lines: 80,
    functions: 90
  },
  './src/logger.ts': {
    statements: 100,
    branches: 100,
    lines: 100,
    functions: 100
  }
}
```

## Test Categories

### Unit Tests

Unit tests focus on testing individual components in isolation. Located in `tests/unit/`.

### Integration Tests

Integration tests verify that different components work together correctly. Located in `tests/integration/`.

### Performance Tests

Performance tests ensure the application meets performance requirements. Located in `tests/performance/`.

### Security Tests

Security tests check for security vulnerabilities. Located in `tests/security/`.

## Running Tests

```bash
# Run all tests
npm test

# Run tests with coverage report
npm test -- --coverage

# Run specific test categories
npm run test:unit
npm run test:integration
npm run test:security
npm run test:performance

# Run all test types (pre-commit)
npm run test:all

# Run a specific test file
npm test -- tests/unit/app-core.test.ts

# Run tests without coverage (faster)
npx jest --no-coverage

# Run tests and detect open handles
npm test -- --detectOpenHandles
```

## Best Practices

1. **Use Real Implementations**: Test the actual code, not mocks of the code under test.

2. **Mock External Dependencies Only**: Only mock fs, network calls, and external APIs:
   ```javascript
   // Good: mock dependencies, import real module
   jest.mock('fs');
   jest.mock('http');
   const solidPodService = require('../../src/solid-pod-service');

   // Bad: mocking the module under test
   jest.mock('../../src/solid-pod-service');
   ```

3. **Proper Test Cleanup**: Always clean up resources:
   ```javascript
   afterEach(() => {
     jest.resetModules();
     jest.clearAllMocks();
   });
   ```

4. **Direct Function Testing**: Test exported functions directly rather than through HTTP requests when testing unit behavior:
   ```javascript
   const handler = createHomeHandler(mockLogger, mockPodService);
   const req = {};
   const res = { status: jest.fn().mockReturnThis(), render: jest.fn() };
   await handler.homeHandler(req, res);
   expect(res.render).toHaveBeenCalled();
   ```

5. **Coverage-Aware Testing**: Write tests targeting uncovered code paths:
   ```javascript
   it('should handle errors properly', async () => {
     mockDependency.mockRejectedValueOnce(new Error('Test error'));
     await expect(functionUnderTest()).rejects.toThrow('Test error');
   });
   ```

6. **Test Isolation**: Tests must not depend on each other or share mutable state.

7. **Descriptive Test Names**: Use names that clearly indicate what is being tested and what the expected outcome is.

## Coverage Troubleshooting

### Tests Pass but Don't Contribute to Coverage

**Cause**: Mocking the module under test instead of its dependencies.

```javascript
// Problem: this replaces the real module with a mock
jest.mock('../../src/solid-pod-service');

// Fix: mock only external dependencies, then import real module
jest.mock('fs');
jest.mock('http');
const solidPodService = require('../../src/solid-pod-service');
```

### Mock Implementation Shadows Real Code

**Cause**: Custom mock implementations that duplicate production logic.

```javascript
// Problem: reimplements the logic being tested
mockFunction.mockImplementation((arg) => {
  if (arg === 'special') return 'special result';
  return 'default result';
});

// Fix: use real implementation, mock only its dependencies
const realModule = jest.requireActual('../../src/module');
```

### Import Order Issues

Jest hoists `jest.mock()` calls. Always declare mocks before importing the module under test:

```javascript
// First: mock external dependencies
jest.mock('fs');
jest.mock('http');

// Then: import module under test
const solidPodService = require('../../src/solid-pod-service');
```

### "Address Already in Use" in Tests

1. Check for zombie test processes: `ps aux | grep jest`
2. Verify port availability: `lsof -i :3000-3100`
3. Use `--detectOpenHandles` to find tests not closing connections
4. For integration tests, use dynamic port allocation:
   ```typescript
   const port = Math.floor(Math.random() * 10000) + 10000;
   ```

### ESLint max-lines-per-function in Test Files

Test files have a relaxed limit of 1500 lines per function (configured in `.eslintrc.js`).
