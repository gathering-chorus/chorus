/**
 * Origin labels — reflective vs reactive classification for cards.
 * Verifies config, SDK, and Vikunja label integrity.
 */
import { LABELS, loadEnv, GATHERING } from '../src/config';
import { BoardClient } from '../src/client';

describe('Origin label config', () => {
  test('LABELS.origin exists with reflective and reactive', () => {
    expect(LABELS.origin).toBeDefined();
    expect(LABELS.origin['reflective']).toBe(87);
    expect(LABELS.origin['reactive']).toBe(88);
  });

  test('origin label IDs are unique across all categories', () => {
    const allIds: number[] = [];
    for (const cat of Object.values(LABELS)) {
      allIds.push(...Object.values(cat as Record<string, number>));
    }
    const counts = new Map<number, number>();
    for (const id of allIds) {
      counts.set(id, (counts.get(id) || 0) + 1);
    }
    expect(counts.get(87)).toBe(1);
    expect(counts.get(88)).toBe(1);
  });
});

describe('Origin label in Vikunja', () => {
  let client: BoardClient;
  let allLabels: Array<{ id: number; title: string }>;

  beforeAll(async () => {
    const env = loadEnv();
    client = new BoardClient(env.url, env.token, GATHERING);
    allLabels = await client.listLabels();
  });

  test('origin:reflective label exists in Vikunja with ID 87', () => {
    const label = allLabels.find(l => l.id === 87);
    expect(label).toBeDefined();
    expect(label!.title).toBe('origin:reflective');
  });

  test('origin:reactive label exists in Vikunja with ID 88', () => {
    const label = allLabels.find(l => l.id === 88);
    expect(label).toBeDefined();
    expect(label!.title).toBe('origin:reactive');
  });
});

describe('SDK accepts origin as set key', () => {
  test('setCard does not throw on origin key', async () => {
    // Import dynamically so config changes are picked up
    const { setCard } = await import('../src/sdk');
    const env = loadEnv();
    const client = new BoardClient(env.url, env.token, GATHERING);

    // Use a Won't Do test card to avoid mutating real data
    // This tests that the key is accepted, not that the tag sticks
    await expect(
      setCard(client, 1794, { origin: 'reflective' })
    ).resolves.not.toThrow();
  });
});
