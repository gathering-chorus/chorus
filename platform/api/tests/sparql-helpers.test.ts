import { escSparql, icdSlug } from '../src/sparql-helpers';

describe('escSparql', () => {
  it('passes through a plain string unchanged', () => {
    expect(escSparql('hello world')).toBe('hello world');
  });

  it('escapes backslashes first to avoid double-escape', () => {
    expect(escSparql('a\\b')).toBe('a\\\\b');
  });

  it('escapes double quotes', () => {
    expect(escSparql('she said "hi"')).toBe('she said \\"hi\\"');
  });

  it('escapes newlines to literal \\n', () => {
    expect(escSparql('line1\nline2')).toBe('line1\\nline2');
  });

  it('strips carriage returns entirely', () => {
    expect(escSparql('line1\r\nline2')).toBe('line1\\nline2');
  });

  it('handles all escape sequences together', () => {
    expect(escSparql('a\\"b\nc\r')).toBe('a\\\\\\"b\\nc');
  });
});

describe('icdSlug', () => {
  it('lowercases the input', () => {
    expect(icdSlug('Domain')).toBe('domain');
  });

  it('replaces non-alphanumeric runs with a single dash', () => {
    expect(icdSlug('hello world')).toBe('hello-world');
    expect(icdSlug('hello   world')).toBe('hello-world');
  });

  it('collapses multiple separators into one dash', () => {
    expect(icdSlug('foo / bar _ baz')).toBe('foo-bar-baz');
  });

  it('strips leading and trailing dashes', () => {
    expect(icdSlug('-foo-')).toBe('foo');
    expect(icdSlug('   foo   ')).toBe('foo');
  });

  it('returns empty string for all-separator input', () => {
    expect(icdSlug('   ')).toBe('');
    expect(icdSlug('---')).toBe('');
  });

  it('preserves digits', () => {
    expect(icdSlug('version 2 alpha')).toBe('version-2-alpha');
  });
});
