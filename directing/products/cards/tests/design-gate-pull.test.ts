/**
 * Design gate in /pull — #1396
 *
 * Verifies the /pull skill includes a design gate that checks
 * domain completeness before allowing WIP entry for new/enhance cards.
 * Uses SKILL.md source inspection — no real cards pulled.
 */
import * as fs from 'fs';
import * as path from 'path';

const SKILL_SRC = fs.readFileSync(
  path.join(__dirname, '../../../../skills/pull/SKILL.md'), 'utf-8'
);

describe('Design gate in /pull skill (#1396)', () => {

  test('Step 4 includes completeness API fetch', () => {
    expect(SKILL_SRC).toMatch(/completeness/);
    expect(SKILL_SRC).toMatch(/lifecycle\.wip/);
  });

  test('design gate blocks type:new and type:enhance when wip.pass is false', () => {
    expect(SKILL_SRC).toMatch(/type:new/);
    expect(SKILL_SRC).toMatch(/type:enhance/);
    expect(SKILL_SRC).toMatch(/BLOCK|block|STOP|stop/);
  });

  test('design gate exempts type:chore, type:swat, type:fix', () => {
    expect(SKILL_SRC).toMatch(/type:chore/);
    expect(SKILL_SRC).toMatch(/type:swat/);
    expect(SKILL_SRC).toMatch(/type:fix/);
    expect(SKILL_SRC).toMatch(/exempt|skip/i);
  });

  test('error message shows missing sections and POST endpoint', () => {
    expect(SKILL_SRC).toMatch(/missing/i);
    expect(SKILL_SRC).toMatch(/POST.*athena|athena.*POST|populate/i);
  });

  test('spine event pull.design_gate.completed is emitted', () => {
    expect(SKILL_SRC).toMatch(/pull\.design_gate\.completed/);
  });
});
