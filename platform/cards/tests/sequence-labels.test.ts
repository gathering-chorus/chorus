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

  test('cards set sequence=X produces sequence:X label on card', async () => {
    const card = await client.view(1794);
    const seqLabels = card.domains.filter(d => d.startsWith('sequence:'));
    const bareSeqNames = Object.keys(LABELS.sequence);
    const bareLabels = card.domains.filter(d => bareSeqNames.includes(d) && !d.includes(':'));

    expect(bareLabels).toEqual([]);
    expect(seqLabels.length).toBeGreaterThanOrEqual(1);
  });
});
