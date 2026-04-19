/**
 * #2228 — cards view truncates auto-generated long comments by default,
 * --verbose shows full.
 *
 * Tests what Jeff sees when running `cards view <id>`:
 *   - blast-radius comment (40+ lines) → one-line summary + verbose hint
 *   - domain-radius comment → one-line summary + verbose hint
 *   - user-authored gate:product-pass comment → full text (unchanged)
 *   - --verbose flag → full text for all comments
 */
import { formatCommentForView } from '../src/cli-view-helpers';

const BLAST_COMMENT = `**Blast Radius** — 104 files, 1 domains

Domains: chorus

**API** (101):
  chorus/platform/api/src/cost-summary.ts
  chorus/platform/api/src/fitness-summary.ts
  chorus/platform/api/src/hooks-summary.ts
  chorus/platform/api/src/jeff-summary.ts
  chorus/platform/api/src/patterns-summary.ts`;

const DOMAIN_RADIUS_COMMENT = `**Domain Radius** — missing context: domain-context-chorus.md

Add domain-context-chorus.md at:
  /Users/jeffbridwell/CascadeProjects/chorus/directing/products/cards/domain-context/`;

const USER_COMMENT = `gate:product-pass — wren. All AC met. Ready for acp.

Details:
- Tests green
- Docs updated`;

describe('#2228 formatCommentForView', () => {
  test('auto-generated Blast Radius comment truncates to first line + verbose hint when not verbose', () => {
    const out = formatCommentForView(BLAST_COMMENT, false);
    const lines = out.split('\n');
    expect(lines[0]).toBe('**Blast Radius** — 104 files, 1 domains');
    expect(out).toMatch(/--verbose/);
    expect(out).not.toContain('chorus/platform/api/src/cost-summary.ts');
  });

  test('auto-generated Domain Radius comment truncates to first line + verbose hint when not verbose', () => {
    const out = formatCommentForView(DOMAIN_RADIUS_COMMENT, false);
    const lines = out.split('\n');
    expect(lines[0]).toContain('**Domain Radius** — missing context');
    expect(out).toMatch(/--verbose/);
    expect(out).not.toContain('Add domain-context-chorus.md at:');
  });

  test('auto-generated comment renders full when verbose=true', () => {
    const out = formatCommentForView(BLAST_COMMENT, true);
    expect(out).toContain('chorus/platform/api/src/cost-summary.ts');
    expect(out).toContain('**Blast Radius**');
    expect(out).not.toMatch(/--verbose/);  // no hint when already verbose
  });

  test('user-authored comment renders full when not verbose — no truncation', () => {
    const out = formatCommentForView(USER_COMMENT, false);
    expect(out).toBe(USER_COMMENT);
    expect(out).toContain('Tests green');
    expect(out).toContain('Docs updated');
  });

  test('user-authored comment renders full when verbose=true', () => {
    const out = formatCommentForView(USER_COMMENT, true);
    expect(out).toBe(USER_COMMENT);
  });

  test('empty comment stays empty regardless of verbose', () => {
    expect(formatCommentForView('', false)).toBe('');
    expect(formatCommentForView('', true)).toBe('');
  });

  test('single-line auto-generated comment does not add hint (no content to hide)', () => {
    const singleLine = '**Blast Radius** — 0 files, 0 domains';
    const out = formatCommentForView(singleLine, false);
    expect(out).toBe(singleLine);
    expect(out).not.toMatch(/--verbose/);
  });
});
