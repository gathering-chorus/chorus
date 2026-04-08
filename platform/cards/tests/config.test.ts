import { GATHERING, SELF, resolveBucket, LABELS } from '../src/config';

describe('Board configs', () => {
  test('Gathering has all required buckets', () => {
    const required = ['later', 'next', 'now', 'blocked', 'done', 'jeff-tickets', 'tech-debt'];
    for (const bucket of required) {
      expect(GATHERING.buckets[bucket]).toBeDefined();
      expect(typeof GATHERING.buckets[bucket]).toBe('number');
    }
  });

  test('Self has all required buckets', () => {
    const required = ['later', 'next', 'now', 'blocked', 'done'];
    for (const bucket of required) {
      expect(SELF.buckets[bucket]).toBeDefined();
      expect(typeof SELF.buckets[bucket]).toBe('number');
    }
  });

  test('Gathering and Self have different project IDs', () => {
    expect(GATHERING.projectId).not.toBe(SELF.projectId);
  });

  test('Gathering and Self have different view IDs', () => {
    expect(GATHERING.viewId).not.toBe(SELF.viewId);
  });

  test('bucketNames reverse-maps every bucket ID', () => {
    for (const [name, id] of Object.entries(GATHERING.buckets)) {
      expect(GATHERING.bucketNames[id]).toBeDefined();
    }
    for (const [name, id] of Object.entries(SELF.buckets)) {
      expect(SELF.bucketNames[id]).toBeDefined();
    }
  });
});

describe('resolveBucket', () => {
  test('resolves standard status names for Gathering', () => {
    expect(resolveBucket(GATHERING, 'now')).toBe(GATHERING.buckets['now']);
    expect(resolveBucket(GATHERING, 'Next')).toBe(GATHERING.buckets['next']);
    expect(resolveBucket(GATHERING, 'Later')).toBe(GATHERING.buckets['later']);
    expect(resolveBucket(GATHERING, 'Done')).toBe(GATHERING.buckets['done']);
    expect(resolveBucket(GATHERING, 'Blocked')).toBe(GATHERING.buckets['blocked']);
  });

  test('resolves aliases', () => {
    expect(resolveBucket(GATHERING, 'In Progress')).toBe(GATHERING.buckets['now']);
    expect(resolveBucket(GATHERING, 'ip')).toBe(GATHERING.buckets['now']);
    expect(resolveBucket(GATHERING, 'doing')).toBe(GATHERING.buckets['now']);
    expect(resolveBucket(GATHERING, 'ready')).toBe(GATHERING.buckets['next']);
    expect(resolveBucket(GATHERING, 'todo')).toBe(GATHERING.buckets['later']);
  });

  test('resolves jeff-tickets and tech-debt aliases', () => {
    expect(resolveBucket(GATHERING, 'jt')).toBe(GATHERING.buckets['jeff-tickets']);
    expect(resolveBucket(GATHERING, 'td')).toBe(GATHERING.buckets['tech-debt']);
    expect(resolveBucket(GATHERING, 'Jeff Tickets')).toBe(GATHERING.buckets['jeff-tickets']);
    expect(resolveBucket(GATHERING, 'Tech Debt')).toBe(GATHERING.buckets['tech-debt']);
  });

  test('resolves standard names for Self', () => {
    expect(resolveBucket(SELF, 'now')).toBe(SELF.buckets['now']);
    expect(resolveBucket(SELF, 'next')).toBe(SELF.buckets['next']);
    expect(resolveBucket(SELF, 'done')).toBe(SELF.buckets['done']);
  });

  test("resolves won't do bucket and aliases", () => {
    expect(resolveBucket(GATHERING, "won't do")).toBe(13);
    expect(resolveBucket(GATHERING, 'wd')).toBe(13);
    expect(resolveBucket(GATHERING, 'dup')).toBe(13);
    expect(resolveBucket(GATHERING, 'killed')).toBe(13);
    expect(resolveBucket(GATHERING, 'wontdo')).toBe(13);
    expect(resolveBucket(GATHERING, 'not doing')).toBe(13);
  });

  test('throws on unknown status', () => {
    expect(() => resolveBucket(GATHERING, 'invalid')).toThrow('Unknown status');
  });

  test('is case-insensitive', () => {
    expect(resolveBucket(GATHERING, 'NOW')).toBe(GATHERING.buckets['now']);
    expect(resolveBucket(GATHERING, 'DONE')).toBe(GATHERING.buckets['done']);
    expect(resolveBucket(SELF, 'LATER')).toBe(SELF.buckets['later']);
  });
});

describe('Labels', () => {
  test('owner labels cover all roles', () => {
    expect(LABELS.owner['jeff']).toBeDefined();
    expect(LABELS.owner['wren']).toBeDefined();
    expect(LABELS.owner['silas']).toBeDefined();
    expect(LABELS.owner['kade']).toBeDefined();
  });

  test('priority labels cover P1-P3', () => {
    expect(LABELS.priority['P1']).toBeDefined();
    expect(LABELS.priority['P2']).toBeDefined();
    expect(LABELS.priority['P3']).toBeDefined();
  });

  test('all label IDs are unique numbers', () => {
    const allIds = [
      ...Object.values(LABELS.owner),
      ...Object.values(LABELS.priority),
      ...Object.values(LABELS.domain),
    ];
    const unique = new Set(allIds);
    expect(unique.size).toBe(allIds.length);
    for (const id of allIds) {
      expect(typeof id).toBe('number');
    }
  });
});
