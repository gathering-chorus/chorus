// @test-type: unit
// #3707 — the athena-health query catalog is a promise surface: every path it
// advertises must be servable. #3702 retired /api/athena/steps (410 → owl-api
// /valuestreams); a catalog that still lists it points readers at a dead door.
import { DEFAULT_QUERIES } from '../src/handlers/athena-health';

describe('athena-health catalog honesty (#3707)', () => {
  it('does not advertise the retired v1 steps endpoint (#3702)', () => {
    const paths = DEFAULT_QUERIES.map((q) => q.path);
    expect(paths).not.toContain('/api/athena/steps');
  });
});
