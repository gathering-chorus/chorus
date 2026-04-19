import { createSubdomainResolver, isTestFile } from '../src/subdomain-resolver';

describe('isTestFile', () => {
  it('recognizes /tests/ path segment', () => {
    expect(isTestFile('/proj/tests/foo.ts')).toBe(true);
    expect(isTestFile('/proj/test/foo.ts')).toBe(true);
  });

  it('recognizes __tests__ fixture dir', () => {
    expect(isTestFile('/proj/__tests__/foo.ts')).toBe(true);
  });

  it('recognizes .test. and .spec. name patterns', () => {
    expect(isTestFile('/proj/src/foo.test.ts')).toBe(true);
    expect(isTestFile('/proj/src/foo.spec.ts')).toBe(true);
  });

  it('recognizes Rust _test.rs and .feature files', () => {
    expect(isTestFile('/proj/src/bar_test.rs')).toBe(true);
    expect(isTestFile('/proj/features/login.feature')).toBe(true);
  });

  it('recognizes .bats shell tests', () => {
    expect(isTestFile('/proj/scripts/check.bats')).toBe(true);
  });

  it('returns false for non-test source files', () => {
    expect(isTestFile('/proj/src/foo.ts')).toBe(false);
    expect(isTestFile('/proj/README.md')).toBe(false);
  });
});

describe('createSubdomainResolver', () => {
  it('passes through an explicit -domain name unchanged', async () => {
    const sparql = jest.fn();
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('seeds-domain')).toBe('seeds-domain');
    expect(sparql).not.toHaveBeenCalled();
  });

  it('passes through an explicit -service name unchanged', async () => {
    const sparql = jest.fn();
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('tests-service')).toBe('tests-service');
    expect(sparql).not.toHaveBeenCalled();
  });

  it('lowercases the input for -domain / -service passthrough', async () => {
    const sparql = jest.fn();
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('Seeds-Domain')).toBe('seeds-domain');
  });

  it('returns -domain when the ASK query returns true', async () => {
    const sparql = jest.fn(async () => ({ boolean: true }));
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('seeds')).toBe('seeds-domain');
    expect(sparql).toHaveBeenCalledTimes(1);
  });

  it('falls back to -service when the ASK query returns false', async () => {
    const sparql = jest.fn(async () => ({ boolean: false }));
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('tests')).toBe('tests-service');
  });

  it('falls back to -service when the ASK query throws', async () => {
    const sparql = jest.fn(async () => { throw new Error('Fuseki down'); });
    const resolve = createSubdomainResolver({ sparql });
    expect(await resolve('anything')).toBe('anything-service');
  });
});
