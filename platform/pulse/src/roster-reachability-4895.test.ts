// @test-type: unit — presence rows and actor facts in memory
// @domain: awareness
// @card: #4895
// @owner: silas
// #4895 AC5 — the room shows a role that cannot be reached. The roster reads pulse's
// actor-state; each actor now carries the reachability of the Presence delivery uses.
import { reachabilityFromPresence } from './presence-target';
import { deriveActorState } from './actor-state';

const runs = [{ name: 'abby-normal-run-1a1ac823a4b' }, { name: 'kade-run-1a193a88380' }, { name: 'wren-run-old', runEndedAt: '2026-11-05T10:00:00Z' }];
const presences = [
  { name: 'abby-normal-presence-a', presenceOf: 'session-run-abby-normal-run-1a1ac823a4b', pane: '%16', reachability: 'unreachable', checkedAt: '2026-11-06T14:00:00Z' },
  { name: 'kade-presence-b', presenceOf: 'session-run-kade-run-1a193a88380', pane: '%1', reachability: 'reachable', checkedAt: '2026-11-06T14:37:00Z' },
  { name: 'wren-presence-c', presenceOf: 'session-run-wren-run-old', pane: '%9', reachability: 'reachable', checkedAt: '2026-11-05T09:00:00Z' },
];

describe('#4895 the roster shows who cannot be reached', () => {
  it('the delivery Presence reachability is what the roster reads', () => {
    expect(reachabilityFromPresence(presences, runs, 'abby-normal')).toBe('unreachable');
    expect(reachabilityFromPresence(presences, runs, 'kade')).toBe('reachable');
  });

  it('NEGATIVE PROOF: a Presence of an ENDED run never reads reachable; no live run is offline', () => {
    expect(reachabilityFromPresence(presences, runs, 'wren')).toBe('offline');
    expect(reachabilityFromPresence(presences, runs, 'borg-queen')).toBe('offline');
  });

  it('actor-state carries each actor its reachability, and none when unread', () => {
    const out = deriveActorState({ now: new Date('2026-11-06T15:00:00Z'), principals: ['principal-abby-normal', 'principal-kade'],
      reachability: { 'abby-normal': 'unreachable', kade: 'reachable' } });
    expect(out.find((a) => a.aboutPrincipal === 'principal-abby-normal')?.reachability).toBe('unreachable');
    expect(out.find((a) => a.aboutPrincipal === 'principal-kade')?.reachability).toBe('reachable');
    const unread = deriveActorState({ now: new Date('2026-11-06T15:00:00Z'), principals: ['principal-kade'] });
    expect(unread[0].reachability).toBeUndefined();
  });
});
