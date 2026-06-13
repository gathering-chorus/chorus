/**
 * #3409 — /cw skill conformance. /cw is the thin runner skin over the chorus_werk
 * pipeline verb (the Building value stream), matching the werk-verb skill pattern
 * (ADR-032/037): user-invocable, one typed MCP verb, no raw shell/act.
 */
import * as fs from 'fs';
import * as path from 'path';

const SKILL = path.resolve(__dirname, '..', '..', '..', 'skills', 'cw', 'SKILL.md');

describe('#3409 — /cw skill contract', () => {
  test('SKILL.md exists with user-invocable frontmatter', () => {
    expect(fs.existsSync(SKILL)).toBe(true);
    const md = fs.readFileSync(SKILL, 'utf-8');
    expect(md).toMatch(/^---[\s\S]*name:\s*cw[\s\S]*user-invocable:\s*true[\s\S]*---/);
  });

  test('invokes the chorus_werk MCP verb (the runner contract)', () => {
    const md = fs.readFileSync(SKILL, 'utf-8');
    expect(md).toContain('mcp__chorus-api__chorus_werk');
  });

  test('documents the GO=accept guard (never go without explicit human go)', () => {
    const md = fs.readFileSync(SKILL, 'utf-8').toLowerCase();
    expect(md).toContain('go: true');
    expect(md).toMatch(/never pass `?go/);
  });

  test('does NOT shell out to act or raw werk leaf verbs from the skill', () => {
    const md = fs.readFileSync(SKILL, 'utf-8');
    expect(md).not.toMatch(/```[\s\S]*\bbash .*act\b/);
    expect(md).not.toMatch(/\$\(\s*act\b/);
  });
});
