// @test-type: unit — pure pulse logic on the real rows; no live services
// @domain: messages
// @card: #4889
// @owner: silas
/* eslint-disable sonarjs/no-duplicate-string -- the stored rows, verbatim */
// #4889 (folded #4891) — src/announce-boundary: a run-finish wake is never dup-class
// suppressed. Measured 11-05: run 3's failure wake (06:16) and run 4's (06:29) had one
// signature; run 4's was suppressed and reached Silas at 06:58, when Jeff was asking.

import { decide, AnnounceState } from './announce-boundary';

const COOLDOWN = 30 * 60_000;
const at = (hhmm: string) => Date.parse(`2026-11-05T${hhmm}:00-05:00`);
const wake = (hhmm: string, run: string) =>
  `[alert for silas · from system | 2026-11-05 ${hhmm} Boston] #4889 cicd failed · /cws 4889\n\`\`\`\ncicd failed at test-pipeline: test-verdict: werk-test: RED (exit 1) · dagu ${run}\n\`\`\``;

describe('#4889 a run-finish wake always reaches its owner', () => {
  it('the 06:16 and 06:29 failures of the same step both deliver', () => {
    const st = new AnnounceState(COOLDOWN);
    expect(decide('silas', 'silas', wake('06:16', '034sHMhiqg3TqppDbt8KmN'), at('06:16'), st, 'run-finished').deliver).toBe(true);
    const second = decide('silas', 'silas', wake('06:29', '034sHhC48jXQrC4KpmjYrt'), at('06:29'), st, 'run-finished');
    expect(second.deliver).toBe(true);
    expect(second.suppressReason).toBeUndefined();
  });

  it('NEGATIVE PROOF: the same pair without the wake mark collides — run 4 is dup-class suppressed', () => {
    const st = new AnnounceState(COOLDOWN);
    expect(decide('silas', 'silas', wake('06:16', '034sHMhiqg3TqppDbt8KmN'), at('06:16'), st).deliver).toBe(true);
    const second = decide('silas', 'silas', wake('06:29', '034sHhC48jXQrC4KpmjYrt'), at('06:29'), st);
    expect(second).toMatchObject({ deliver: false, suppressReason: 'dup-class' });
  });

  it('NEGATIVE PROOF: the mark is a field, not text — the words "run-finished" in an alert do not exempt it', () => {
    const st = new AnnounceState(COOLDOWN);
    const text = '[alert for silas · from system | 2026-11-05 06:16 Boston] run-finished: deep-health red';
    decide('silas', 'silas', text, at('06:16'), st);
    expect(decide('silas', 'silas', text, at('06:20'), st).suppressReason).toBe('dup-class');
  });

  it('an unknown wake value is deduped like any alert', () => {
    const st = new AnnounceState(COOLDOWN);
    decide('silas', 'silas', wake('06:16', 'a'), at('06:16'), st, 'something-else');
    expect(decide('silas', 'silas', wake('06:29', 'b'), at('06:29'), st, 'something-else').suppressReason).toBe('dup-class');
  });

  it('with no cooldown read (door down, row missing) nothing is deduped', () => {
    const st = new AnnounceState(undefined);
    decide('silas', 'silas', wake('06:16', 'a'), at('06:16'), st);
    expect(decide('silas', 'silas', wake('06:29', 'b'), at('06:29'), st).deliver).toBe(true);
  });
});
