// @test-type: unit — inferred by chorus-crawl --classify (hermetic) (#4538)
// @domain: cards — placed by chorus-crawl's domain rules (#4538)
import { LABELS } from '../src/config';

describe('Athena sequence label', () => {
  test('LABELS.sequence includes athena with label ID 137', () => {
    expect(LABELS.sequence.athena).toBe(137);
  });
});
