/**
 * #3415 — design-system contract. system.css is the portable token base (two
 * themes on one base, audited tokens, first-class print); the specimen + the
 * architecture doc both build ON it (dogfood). Asserts the contract holds so a
 * future edit can't silently drop a theme, a token group, or the print rules.
 */
import * as fs from 'fs';
import * as path from 'path';

const ROOT = path.resolve(__dirname, '..', '..', '..');
const CSS = path.join(ROOT, 'platform', 'api', 'public', 'css', 'system.css');
const SPEC = path.join(ROOT, 'platform', 'api', 'public', 'ui-system.html');
const DOC = path.join(ROOT, 'designing', 'docs', 'chorus-ui-architecture.html');

describe('#3415 — design system contract', () => {
  test('system.css defines both themes on one token base', () => {
    expect(fs.existsSync(CSS)).toBe(true);
    const css = fs.readFileSync(CSS, 'utf8');
    expect(css).toMatch(/\.theme-light\s*\{/);
    expect(css).toMatch(/\.theme-dark\s*\{/);
  });

  test('system.css carries the audited token groups (spacing, stage, role)', () => {
    const css = fs.readFileSync(CSS, 'utf8');
    expect(css).toContain('--space-3');
    expect(css).toContain('--stage-building');
    expect(css).toContain('--role-kade');
  });

  test('print is first-class (@media print in the token sheet)', () => {
    const css = fs.readFileSync(CSS, 'utf8');
    expect(css).toMatch(/@media\s+print/);
  });

  test('portable — no chorus-specific hardcoding (inheritable capability)', () => {
    const css = fs.readFileSync(CSS, 'utf8');
    // tokens/components only — no absolute localhost URLs or role-session paths baked in
    expect(css).not.toMatch(/localhost:\d/);
    expect(css).not.toMatch(/\/Users\//);
  });

  test('specimen + architecture doc both build ON system.css (dogfood)', () => {
    expect(fs.readFileSync(SPEC, 'utf8')).toContain('/css/system.css');
    const doc = fs.readFileSync(DOC, 'utf8');
    expect(doc).toContain('/css/system.css');
    expect(doc).toMatch(/class="theme-light"/);
  });
});
