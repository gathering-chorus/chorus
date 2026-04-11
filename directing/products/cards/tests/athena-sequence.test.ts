import { LABELS } from '../src/config';

describe('Athena sequence label', () => {
  test('LABELS.sequence includes athena with label ID 137', () => {
    expect(LABELS.sequence.athena).toBe(137);
  });
});
