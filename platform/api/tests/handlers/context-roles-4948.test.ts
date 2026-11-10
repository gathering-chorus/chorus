// @test-type: unit — loggedInRoles and the sessions door queries, against fixture rows; no live door.
// @domain: roles
/**
 * #4948 — Jeff 2026-11-10 14:34: "i want a session that is effectively bounded by
 * chorus-principal login and logout". An agent's session is live from login until
 * logout sets endedAt, however quiet the role is. Measured 14:32: a role quiet for an
 * hour lost its tile (expiresAt passed and lastSeenAt older than 1h), and the door
 * query filtered on expiresAt, so a row with none never arrived (Silas, pair 4948).
 */
import { agentSessionsQuery, liveSessionsQuery, loggedInRoles, mergeSessionListings } from '../../src/handlers/context-roles';

const DOOR = { data: [
  { name: 'jeff', roleKind: 'human', rolePriority: '0' },
  { name: 'silas', roleKind: 'agent', rolePriority: '2' },
  { name: 'wren', roleKind: 'agent', rolePriority: '3' },
] };
const NOW = Date.parse('2026-11-10T21:00:00Z');
const TWO_HOURS_AGO = '2026-11-10T19:00:00Z';

describe('#4948 an agent session lasts from login to logout', () => {
  it('an agent row with no expiresAt, quiet for 2h, keeps its tile', () => {
    const rows = { data: [{ name: 'wren-s', ownedBy: 'principal-wren', sessionState: 'open', channel: 'pane', startedAt: '2026-11-09T16:22:00Z', lastSeenAt: TWO_HOURS_AGO }] };
    expect(loggedInRoles(rows, DOOR, NOW).map((r) => r.name)).toEqual(['wren']);
  });

  it('an agent row already open with a lapsed expiresAt, quiet for 2h, keeps its tile (no store write needed)', () => {
    const rows = { data: [{ name: 'silas-s', ownedBy: 'principal-silas', sessionState: 'open', channel: 'pane', lastSeenAt: TWO_HOURS_AGO, expiresAt: '2026-11-10T19:10:00Z' }] };
    expect(loggedInRoles(rows, DOOR, NOW).map((r) => r.name)).toEqual(['silas']);
  });

  it('NEGATIVE PROOF: the same agent row with endedAt set (logout) has no tile', () => {
    const rows = { data: [{ name: 'wren-s', ownedBy: 'principal-wren', sessionState: 'open', channel: 'pane', lastSeenAt: TWO_HOURS_AGO, endedAt: '2026-11-10T20:00:00Z' }] };
    expect(loggedInRoles(rows, DOOR, NOW)).toEqual([]);
    const closed = { data: [{ name: 'wren-s', ownedBy: 'principal-wren', sessionState: 'closed', channel: 'pane', endedAt: '2026-11-10T20:00:00Z' }] };
    expect(loggedInRoles(closed, DOOR, NOW)).toEqual([]);
  });

  it("NEGATIVE PROOF: a person's browser sign-in still ends at its expiry (Jeff's door sets it)", () => {
    const rows = { data: [{ name: 'jeff-b', ownedBy: 'principal-jeff', sessionState: 'open', channel: 'browser', lastSeenAt: TWO_HOURS_AGO, expiresAt: '2026-11-10T20:00:00Z' }] };
    expect(loggedInRoles(rows, DOOR, NOW)).toEqual([]);
  });
});

describe('#4948 the sessions door queries', () => {
  it('agent sessions are asked for with no expiry filter: open panes only', () => {
    const q = agentSessionsQuery();
    expect(q).toContain('endedAt.absent=true');
    expect(q).toContain('channel=pane');
    expect(q).not.toContain('expiresAt');
  });

  it('the browser query keeps its expiry window', () => {
    expect(liveSessionsQuery(NOW)).toContain('expiresAt.after=');
  });

  it('the two listings merge into one, each row once', () => {
    const a = { data: [{ name: 'wren-s', channel: 'pane' }, { name: 'jeff-b', channel: 'browser' }] };
    const b = { data: [{ name: 'wren-s', channel: 'pane' }, { name: 'silas-s', channel: 'pane' }] };
    expect((mergeSessionListings(a, b).data as Array<{ name: string }>).map((r) => r.name).sort()).toEqual(['jeff-b', 'silas-s', 'wren-s']);
  });

  it('NEGATIVE PROOF: a listing with no data list is refused, never read as empty', () => {
    expect(() => mergeSessionListings({ data: [] }, { error: 'down' })).toThrow(/no data list/);
  });
});
