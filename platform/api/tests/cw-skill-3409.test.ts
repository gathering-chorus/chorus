// @test-type: unit
// @domain: skills
/**
 * #3409 — /cw skill conformance. /cw is the thin skin over chorus-werk, which
 * starts the werk v2 pipeline (#4515; the old act tool is retired): user-invocable,
 * one tool call, no raw shell, dagu or act.
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

  test('invokes the chorus-werk tool (starts the v2 pipeline)', () => {
    const md = fs.readFileSync(SKILL, 'utf-8');
    expect(md).toContain('mcp__chorus-api__chorus-werk(');
  });

  test('the skill never gives the go: the go is the pipeline\'s own step (DEC-048)', () => {
    const md = fs.readFileSync(SKILL, 'utf-8').toLowerCase();
    expect(md).toContain('never gives a go');
    expect(md).not.toContain('go: true');
  });

  test('does NOT shell out to act or raw werk leaf verbs from the skill', () => {
    const md = fs.readFileSync(SKILL, 'utf-8');
    expect(md).not.toMatch(/```[\s\S]*\bbash .*act\b/);
    expect(md).not.toMatch(/\$\(\s*act\b/);
  });
});
