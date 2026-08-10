// @test-type: unit — loads the real base-path module in a DOM-ish harness; no live services, brings its own world.
/**
 * #3798 — Chorus surfaces serve under a configured base path.
 *
 * The move is llug.com/chorus (Jeff, 2026-08-08). cloudflared does not rewrite
 * paths, so the guard strips its own prefix and the app still serves at root —
 * the browser's URL carries /chorus while the app's does not. Relative links
 * break on that gap the moment a page nests; a configured base does not.
 *
 * These tests exercise the REAL module, not a copy, and cover the deep-nest
 * case that is the whole reason the base exists.
 */
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const SRC = fs.readFileSync(path.join(__dirname, '..', 'public', 'js', 'base-path.js'), 'utf-8');

function load(pathname, metaBase) {
  const nodes = [];
  const doc = {
    readyState: 'complete',
    querySelector: (sel) => (sel.includes('chorus-base') && metaBase
      ? { getAttribute: () => metaBase } : null),
    querySelectorAll: () => nodes,
    addEventListener: () => {},
  };
  const win = { location: { pathname }, document: doc };
  const ctx = vm.createContext({ window: win, document: doc });
  vm.runInContext(SRC, ctx);
  return win;
}

describe('#3798 base resolution', () => {
  test('at root today, BASE is empty — behaviour is byte-for-byte what it was', () => {
    const w = load('/');
    expect(w.CHORUS_BASE).toBe('');
    expect(w.basePath('/athena/model.html')).toBe('/athena/model.html');
  });

  test('served under /chorus, links carry the prefix', () => {
    const w = load('/chorus');
    expect(w.CHORUS_BASE).toBe('/chorus');
    expect(w.basePath('/athena/model.html')).toBe('/chorus/athena/model.html');
  });

  test('THE DEEP-NEST CASE relative links get wrong: from /chorus/domains/x, a link to athena is /chorus/athena', () => {
    const w = load('/chorus/domains/principles');
    // relative resolution would give /chorus/domains/athena/model.html — the bug
    expect(w.basePath('/athena/model.html')).toBe('/chorus/athena/model.html');
  });

  test('an explicit meta tag wins over inference', () => {
    const w = load('/', '/chorus');
    expect(w.CHORUS_BASE).toBe('/chorus');
  });

  test('external URLs are untouched', () => {
    const w = load('/chorus');
    expect(w.basePath('https://id.lightlifeurbangardens.com/x')).toBe('https://id.lightlifeurbangardens.com/x');
    expect(w.basePath('//cdn.example/x')).toBe('//cdn.example/x');
  });

  test('a trailing slash in the base does not produce a double slash', () => {
    const w = load('/', '/chorus/');
    expect(w.basePath('/athena')).toBe('/chorus/athena');
  });
});

describe('#3798 NEGATIVE PROOF — the pre-fix shape is detectable', () => {
  const pages = [
    path.join(__dirname, '..', 'public', 'index.html'),
    ...fs.readdirSync(path.join(__dirname, '..', 'public', 'athena'))
      .filter((f) => f.endsWith('.html'))
      .map((f) => path.join(__dirname, '..', 'public', 'athena', f)),
  ];

  test('every Chorus page loads the base-path module — a page without it silently breaks under a prefix', () => {
    const missing = pages.filter((p) => !fs.readFileSync(p, 'utf-8').includes('base-path.js'));
    expect(missing.map((p) => path.basename(p))).toEqual([]);
  });

  test('the check can go red: a page body without the loader is reported', () => {
    // Guards the guard — the assertion above passes vacuously if the matcher
    // cannot see an absence. Prove it can.
    const fake = '<html><head></head><body><a href="/athena">x</a></body></html>';
    expect(fake.includes('base-path.js')).toBe(false);
  });
});
