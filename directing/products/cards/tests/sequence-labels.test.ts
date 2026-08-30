// @test-type: integration:api — reads the LIVE Vikunja label list; gated by RUN_INTEGRATION (jest.config testPathIgnorePatterns)
/**
 * #2024 AC #1 — Sequence labels must have `sequence:` prefix in Vikunja.
 * Verifies that all label IDs in LABELS.sequence correspond to Vikunja labels
 * titled `sequence:<name>`, not bare `<name>`.
 */
import { LABELS, loadEnv, GATHERING } from '../src/config';
import { BoardClient } from '../src/client';

describe('Sequence label naming (#2024 AC #1)', () => {
  let client: BoardClient;
  let allLabels: Array<{ id: number; title: string }>;

  beforeAll(async () => {
    const env = loadEnv();
    client = new BoardClient(env.url, env.token, GATHERING);
    allLabels = await client.listLabels();
  });

  test('all sequence labels in Vikunja have sequence: prefix', () => {
    const seqEntries = Object.entries(LABELS.sequence);
    const errors: string[] = [];

    for (const [name, id] of seqEntries) {
      const label = allLabels.find(l => l.id === id);
      if (!label) {
        errors.push(`sequence "${name}" (ID ${id}): not found in Vikunja`);
        continue;
      }
      const expected = `sequence:${name}`;
      if (label.title !== expected) {
        errors.push(`sequence "${name}" (ID ${id}): titled "${label.title}", expected "${expected}"`);
      }
    }

    expect(errors).toEqual([]);
  });

  // #4030 — the second case here ("cards set sequence=X produces sequence:X
  // label on card") read live card #1794 and asserted it CURRENTLY carries a
  // sequence label. It never ran `set`; it depended on a real card nobody
  // promised to keep tagged, and went red in run 7 (2026-08-29 16:53) on a
  // transient empty read while the label was in place. The behaviour it
  // named — tag passes category + value through to the board — is proven
  // hermetically in sdk-lifecycle.test.ts ("tagCard passes category + value
  // through"). A test that cannot fail for the reason it claims is retired,
  // not softened (#3734).
});
