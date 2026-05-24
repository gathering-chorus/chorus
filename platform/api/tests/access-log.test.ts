// #3058 - the chorus-api access line must carry the request START timestamp so an
// event-loop-block alert at time T can be matched to the request whose
// [start, start+ms] window contains T. Before this, lines had no timestamp and
// every loop-block alert was untraceable (e.g. the 5258ms block at 15:37:52).

import { formatAccessLine } from '../src/access-log';

describe('formatAccessLine (#3058 - traceable access log)', () => {
  it('includes the ISO start timestamp + method/path/status/ms', () => {
    const start = Date.parse('2026-05-24T15:37:52.990Z');
    const line = formatAccessLine(start, 'GET', '/api/chorus/search', 200, 5258);
    expect(line).toBe('[chorus-api] 2026-05-24T15:37:52.990Z GET /api/chorus/search 200 5258ms');
  });

  it('timestamp is ISO-8601 Z so a block at T maps to [start, start+ms]', () => {
    const line = formatAccessLine(Date.now(), 'POST', '/api/chorus/index', 200, 100);
    expect(line).toMatch(
      /^\[chorus-api\] \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z POST \/api\/chorus\/index 200 100ms$/,
    );
  });
});
