// @domain: awareness
// @test-type: unit — deriveActorState over fact fixtures, the endpoint over an in-memory app; no live services
// @card: #4560
import request from 'supertest';
import { deriveActorState, ActorStateService, isTeamRole, type ActorFactInput } from './actor-state';
import { createApp } from './service';
import { MessageStore } from './store';

const P_WREN = 'principal-wren';
const P_ABBY = 'principal-abby-normal';
const P_SILAS = 'principal-silas';
const P_JEFF = 'principal-jeff';
const P_KADE = 'principal-kade';
const ROLE_AGENT = 'agent';
const ROLE_AUTOMATION = 'automation';
const ROLE_HUMAN = 'human';
const ROLE_NAVIGATOR = 'navigator';
const CH_PANE = 'pane';
const CH_BROWSER = 'browser';
const EXP_1300 = '2026-10-16T13:00:00Z';
const SEEN_1218 = '2026-10-16T12:18:00Z';
const SEEN_1219 = '2026-10-16T12:19:00Z';
const STATE_PAIRING = 'pairing';
const STATE_BUILDING = 'building';
const STATE_REVIEWING = 'reviewing';
const STATE_WAITING = 'waiting';
const STATE_AVAILABLE = 'available';
const STATE_AWAY = 'away';
const STATE_OFF = 'off';
const STATE_UNMEASURED = 'unmeasured';
const ACT_ACTING = 'acting';
const SRC_ATHENA_ROLES = 'athena:roles';
const SRC_SESSIONS = 'athena:identity:sessions';
const SRC_LOKI = 'loki:activity';
const ERR_502 = 'HTTP 502 Bad Gateway';

const baseNow = new Date('2026-10-16T12:20:00Z');

describe('Actor State - Pairing and In-Ceremony', () => {
  test('pairing: active-pairs puts both roles in pairing with partner and card', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_WREN, P_ABBY],
      activePairs: [{ card: 4560, navigator: 'wren', driver: 'abby-normal' }],
      wipCards: [{ cardId: 4560, owner: 'wren' }],
      sessions: [
        { id: 's-wren', principal: P_WREN, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
        { id: 's-abby', principal: P_ABBY, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: {
        wren: { lastActionAt: SEEN_1219, recent5mCount: 4 },
        'abby-normal': { lastActionAt: '2026-10-16T12:19:30Z', recent5mCount: 6 },
      },
    };

    const states = deriveActorState(input);
    const wren = states.find((s) => s.aboutPrincipal === P_WREN);
    const abby = states.find((s) => s.aboutPrincipal === P_ABBY);

    expect(wren?.stateValue).toBe(STATE_PAIRING);
    expect(wren?.pairRole).toBe('navigator');
    expect(wren?.pairedWith).toBe(P_ABBY);
    expect(wren?.currentCard).toBe(4560);
    expect(wren?.activity).toBe(ACT_ACTING);

    expect(abby?.stateValue).toBe(STATE_PAIRING);
    expect(abby?.pairRole).toBe('driver');
    expect(abby?.pairedWith).toBe(P_WREN);
    expect(abby?.currentCard).toBe(4560);
    expect(abby?.activity).toBe(ACT_ACTING);
  });

  test('negative proof: role is NOT pairing when no active-pairs entry matches', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS],
      activePairs: [],
      sessions: [
        { id: 's-silas', principal: P_SILAS, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: { silas: { lastActionAt: SEEN_1219, recent5mCount: 2 } },
      wipCards: [{ cardId: 4444, owner: 'silas' }],
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    expect(silas?.stateValue).not.toBe(STATE_PAIRING);
    expect(silas?.pairRole).toBeUndefined();
    expect(silas?.pairedWith).toBeUndefined();
  });

  test('precedence: pairing beats building when role holds both a WIP card and an active pair', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_WREN],
      activePairs: [{ card: 4560, navigator: 'wren', driver: 'abby-normal' }],
      sessions: [
        { id: 's-wren', principal: P_WREN, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: { wren: { lastActionAt: SEEN_1219, recent5mCount: 3 } },
      wipCards: [{ cardId: 4560, owner: 'wren' }],
    };

    const states = deriveActorState(input);
    const wren = states.find((s) => s.aboutPrincipal === P_WREN);
    expect(wren?.stateValue).toBe(STATE_PAIRING);
    expect(wren?.pairRole).toBe('navigator');
  });
});

describe('Actor State - Session Expiry and Liveness', () => {
  test('session expiry: session past expiresAt or >30m inactive never reads live', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [{ principal: P_JEFF, roleKind: 'human' }],
      sessions: [
        { id: 's-exp', principal: P_JEFF, channel: CH_BROWSER, expiresAt: '2026-10-15T12:00:00Z', lastSeenAt: '2026-10-15T11:00:00Z' },
        { id: 's-stale', principal: P_JEFF, channel: CH_BROWSER, expiresAt: EXP_1300, lastSeenAt: '2026-10-16T11:00:00Z' },
      ],
      actions: { jeff: { lastActionAt: '2026-10-16T11:00:00Z', recent5mCount: 0 } },
    };

    const states = deriveActorState(input);
    const jeff = states.find((s) => s.aboutPrincipal === P_JEFF);
    expect(jeff?.sessions.every((sess) => !sess.live)).toBe(true);
    expect(jeff?.stateValue).toBe(STATE_AWAY);
  });

  test('negative proof: unexpired session with recent activity reads live', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_KADE],
      sessions: [
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { kade: { lastActionAt: SEEN_1218, recent5mCount: 3 } },
    };

    const states = deriveActorState(input);
    const kade = states.find((s) => s.aboutPrincipal === P_KADE);
    expect(kade?.sessions.find((s) => s.id === 's-kade')?.live).toBe(true);
  });

  test('Fix 1: closed sessions with endedAt never read live', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_KADE],
      sessions: [
        {
          id: 's-closed',
          principal: P_KADE,
          channel: CH_PANE,
          expiresAt: EXP_1300,
          lastSeenAt: SEEN_1219,
          endedAt: '2026-10-16T12:19:30Z',
          sessionState: 'closed',
        },
      ],
      actions: { kade: { lastActionAt: '2026-10-16T12:19:30Z', recent5mCount: 0 } },
    };

    const states = deriveActorState(input);
    const kade = states.find((s) => s.aboutPrincipal === P_KADE);
    expect(kade?.sessions.find((s) => s.id === 's-closed')?.live).toBe(false);
  });

  test('Fix 2: session with stale lastSeenAt reads live if role acted within 30m', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_ABBY],
      sessions: [
        { id: 's-abby', principal: P_ABBY, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: '2026-10-15T20:10:00Z' },
      ],
      actions: { 'abby-normal': { lastActionAt: '2026-10-16T12:15:00Z', recent5mCount: 4 } },
      wipCards: [{ cardId: 4560, owner: 'abby-normal' }],
    };

    const states = deriveActorState(input);
    const abby = states.find((s) => s.aboutPrincipal === P_ABBY);
    const session = abby?.sessions.find((s) => s.id === 's-abby');
    expect(session?.live).toBe(true);
    expect(session?.lastActionAt).toBe('2026-10-16T12:15:00Z');
  });

  test('Fix 2 negative proof: session with stale lastSeenAt and stale action reads not live', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_ABBY],
      sessions: [
        { id: 's-abby-stale', principal: P_ABBY, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: '2026-10-15T20:10:00Z' },
      ],
      actions: { 'abby-normal': { lastActionAt: '2026-10-16T11:00:00Z', recent5mCount: 0 } },
    };

    const states = deriveActorState(input);
    const abby = states.find((s) => s.aboutPrincipal === P_ABBY);
    expect(abby?.sessions.find((s) => s.id === 's-abby-stale')?.live).toBe(false);
  });
});

describe('Actor State - Waiting and Building', () => {
  test('waiting: owed nudge with expects=reply marks role as waiting', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS],
      sessions: [
        { id: 's-silas', principal: P_SILAS, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: { silas: { lastActionAt: SEEN_1219, recent5mCount: 1 } },
      owedNudges: [
        { from: 'silas', to: 'kade', expects: 'reply', sentAt: '2026-10-16T12:15:00Z', content: 'Did #4540 finish?' },
      ],
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    expect(silas?.stateValue).toBe(STATE_WAITING);
    expect(silas?.waitingOn).toEqual({
      on: P_KADE,
      what: 'reply',
      since: '2026-10-16T12:15:00Z',
    });
  });

  test('building: WIP card and recent action in 5m marks role as building / acting', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_KADE],
      sessions: [
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { kade: { lastActionAt: SEEN_1218, recent5mCount: 5 } },
      wipCards: [{ cardId: 4540, owner: 'kade' }],
    };

    const states = deriveActorState(input);
    const kade = states.find((s) => s.aboutPrincipal === P_KADE);
    expect(kade?.stateValue).toBe(STATE_BUILDING);
    expect(kade?.currentCard).toBe(4540);
    expect(kade?.activity).toBe(ACT_ACTING);
  });
});

describe('Actor State - Reviewing and Uncarded', () => {
  test('reviewing: requires a reviewed card; person acting with no card does not read reviewing', () => {
    const reviewingInput: ActorFactInput = {
      now: baseNow,
      principals: [{ principal: P_JEFF, roleKind: 'human' }],
      sessions: [
        { id: 's-jeff-live', principal: P_JEFF, channel: CH_BROWSER, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { jeff: { lastActionAt: SEEN_1218, recent5mCount: 2 } },
      reviewCards: [{ cardId: 4560, reviewer: 'jeff', peer: 'abby-normal' }],
    };

    const statesReviewing = deriveActorState(reviewingInput);
    const jeffReviewing = statesReviewing.find((s) => s.aboutPrincipal === P_JEFF);
    expect(jeffReviewing?.stateValue).toBe(STATE_REVIEWING);
    expect(jeffReviewing?.currentCard).toBe(4560);

    const noCardInput: ActorFactInput = {
      now: baseNow,
      principals: [{ principal: P_JEFF, roleKind: 'human' }],
      sessions: [
        { id: 's-jeff-live', principal: P_JEFF, channel: CH_BROWSER, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { jeff: { lastActionAt: SEEN_1218, recent5mCount: 2 } },
    };

    const statesNoCard = deriveActorState(noCardInput);
    const jeffNoCard = statesNoCard.find((s) => s.aboutPrincipal === P_JEFF);
    expect(jeffNoCard?.stateValue).not.toBe(STATE_REVIEWING);
  });

  test('Fix 4: agent alive with no card and no owed nudge does not manufacture waitingOn or building', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS],
      sessions: [
        { id: 's-silas-live', principal: P_SILAS, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { silas: { lastActionAt: SEEN_1218, recent5mCount: 1 } },
      wipCards: [],
      owedNudges: [],
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    expect(silas?.stateValue).not.toBe(STATE_BUILDING);
    expect(silas?.waitingOn).toBeUndefined();
  });
});

describe('Actor State - Dynamic Principals and Activity', () => {
  test('Fix 5: roleKind from roles door determines human vs agent state dynamically', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [
        { principal: 'principal-sarah', roleKind: 'human' },
        { principal: 'principal-helper-bot', roleKind: 'agent' },
      ],
      sessions: [],
      actions: {},
    };

    const states = deriveActorState(input);
    const sarah = states.find((s) => s.aboutPrincipal === 'principal-sarah');
    const helper = states.find((s) => s.aboutPrincipal === 'principal-helper-bot');

    expect(sarah?.stateValue).toBe(STATE_AWAY);
    expect(helper?.stateValue).toBe(STATE_OFF);
  });

  test('Fix 6: Loki activity reports acting for 5m, idle for 30m, and unmeasured when down', () => {
    const downInput: ActorFactInput = {
      now: baseNow,
      principals: [P_KADE],
      sessions: [
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: {},
      unmeasured: [{ source: SRC_LOKI, reason: 'connection refused' }],
    };

    const downStates = deriveActorState(downInput);
    const kadeDown = downStates.find((s) => s.aboutPrincipal === P_KADE);
    expect(kadeDown?.activity).toBe(STATE_UNMEASURED);

    const actingInput: ActorFactInput = {
      now: baseNow,
      principals: [P_KADE],
      sessions: [
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 },
      ],
      actions: { kade: { recent5mCount: 3, lastActionAt: SEEN_1218 } },
    };

    const actingStates = deriveActorState(actingInput);
    const kadeActing = actingStates.find((s) => s.aboutPrincipal === P_KADE);
    expect(kadeActing?.activity).toBe(ACT_ACTING);
  });
});

describe('Actor State - Door Failures and Unmeasured Fallbacks', () => {
  test('unmeasured: unreadable door reports unmeasured by name', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS],
      unmeasured: [{ source: SRC_SESSIONS, reason: 'connection refused' }],
      sessions: [],
      actions: {},
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    expect(silas?.unmeasuredSource).toEqual([
      { source: SRC_SESSIONS, reason: 'connection refused' },
    ]);
  });

  test('Fix 3: sessions door down yields stateValue unmeasured, never off or away', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS, P_JEFF],
      unmeasured: [{ source: SRC_SESSIONS, reason: 'connection refused' }],
      sessions: [],
      actions: {},
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    const jeff = states.find((s) => s.aboutPrincipal === P_JEFF);

    expect(silas?.stateValue).toBe(STATE_UNMEASURED);
    expect(jeff?.stateValue).toBe(STATE_UNMEASURED);
  });

  test('roles door down: reports stateValue unmeasured for every actor', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_SILAS, P_JEFF],
      unmeasured: [{ source: SRC_ATHENA_ROLES, reason: 'HTTP 502' }],
      sessions: [],
      actions: {},
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    const jeff = states.find((s) => s.aboutPrincipal === P_JEFF);

    expect(silas?.stateValue).toBe(STATE_UNMEASURED);
    expect(jeff?.stateValue).toBe(STATE_UNMEASURED);
  });
});

describe('Actor State Service & API Endpoint', () => {
  test('ActorStateService: caches derived states and serves synchronously from cache', async () => {
    const mockFetcher = jest.fn().mockResolvedValue({
      now: baseNow,
      principals: [P_KADE],
      sessions: [{ id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1218 }],
      actions: { kade: { lastActionAt: SEEN_1218, recent5mCount: 2 } },
      wipCards: [{ cardId: 4540, owner: 'kade' }],
    });

    const service = new ActorStateService(mockFetcher, 15000);
    expect(service.getActorStates().actors).toEqual([]);

    await service.refresh();
    const state = service.getActorStates();
    expect(state.actors.length).toBe(1);
    expect(state.actors[0].aboutPrincipal).toBe(P_KADE);
    expect(state.actors[0].stateValue).toBe(STATE_BUILDING);
    expect(state.unmeasured).toEqual([]);

    service.stop();
  });

  test('GET /api/actor-state returns cached actor states from pulse service with single envelope shape', async () => {
    const store = new MessageStore(':memory:');
    const mockService = {
      getActorStates: () => ({
        derivedAt: baseNow.toISOString(),
        actors: [
          {
            aboutPrincipal: P_KADE,
            stateValue: 'building' as const,
            stateSource: 'observed' as const,
            activity: 'acting' as const,
            currentCard: 4540,
            observedAt: baseNow.toISOString(),
            sessions: [],
          },
        ],
        unmeasured: [],
      }),
    };

    const app = createApp(store, undefined, undefined, mockService);
    const res = await request(app).get('/api/actor-state');
    expect(res.status).toBe(200);
    expect(typeof res.body.derivedAt).toBe('string');
    expect(Array.isArray(res.body.actors)).toBe(true);
    expect(res.body.actors.length).toBe(1);
    expect(res.body.actors[0].aboutPrincipal).toBe(P_KADE);
    expect(res.body.actors[0].stateValue).toBe(STATE_BUILDING);
    expect(Array.isArray(res.body.unmeasured)).toBe(true);
    expect(res.body.unmeasured).toEqual([]);
  });
});

describe('Actor State - Roles Door Down', () => {
  test('roles door down after successful fetch keeps last known principals, every one unmeasured', async () => {
    let tick = 0;
    const mockFetcher = jest.fn().mockImplementation(() => {
      tick++;
      if (tick === 1) {
        return Promise.resolve({
          now: baseNow,
          principals: [
            { principal: P_KADE, roleKind: 'agent' },
            { principal: P_JEFF, roleKind: 'human' },
          ],
          sessions: [],
          actions: {},
        });
      }
      return Promise.resolve({
        now: baseNow,
        principals: [],
        unmeasured: [{ source: SRC_ATHENA_ROLES, reason: ERR_502 }],
        sessions: [],
        actions: {},
      });
    });

    const service = new ActorStateService(mockFetcher, 15000);
    await service.refresh();
    const tick1State = service.getActorStates();
    expect(tick1State.actors.length).toBe(2);

    await service.refresh();
    const tick2State = service.getActorStates();
    expect(typeof tick2State.derivedAt).toBe('string');
    expect(tick2State.actors.length).toBe(2);
    expect(tick2State.actors.every((s) => s.stateValue === STATE_UNMEASURED)).toBe(true);
    expect(tick2State.unmeasured.some((u) => u.source === SRC_ATHENA_ROLES)).toBe(true);
  });

  test('roles door down when none ever known -> GET /api/actor-state returns single shape with actors: [] and unmeasured naming why', async () => {
    const mockFetcher = jest.fn().mockResolvedValue({
      now: baseNow,
      principals: [],
      unmeasured: [{ source: SRC_ATHENA_ROLES, reason: ERR_502 }],
      sessions: [],
      actions: {},
    });

    const service = new ActorStateService(mockFetcher, 15000);
    await service.refresh();

    const store = new MessageStore(':memory:');
    const app = createApp(store, undefined, undefined, service);
    const res = await request(app).get('/api/actor-state');

    expect(res.status).toBe(200);
    expect(typeof res.body.derivedAt).toBe('string');
    expect(Array.isArray(res.body.actors)).toBe(true);
    expect(res.body.actors).toEqual([]);
    expect(Array.isArray(res.body.unmeasured)).toBe(true);
    expect(res.body.unmeasured.length).toBeGreaterThan(0);
    expect(res.body.unmeasured[0].source).toBe(SRC_ATHENA_ROLES);
  });
});

describe('Actor State - Stale Pairs and Missing Partner', () => {
  test('stale pair: card not in WIP is ignored so actors do not read pairing', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_WREN, P_KADE],
      activePairs: [{ card: 1778, navigator: 'wren', driver: 'kade' }],
      sessions: [
        { id: 's-wren', principal: P_WREN, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: {
        wren: { lastActionAt: SEEN_1219, recent5mCount: 1 },
        kade: { lastActionAt: SEEN_1219, recent5mCount: 1 },
      },
      wipCards: [],
    };

    const states = deriveActorState(input);
    const wren = states.find((s) => s.aboutPrincipal === P_WREN);
    const kade = states.find((s) => s.aboutPrincipal === P_KADE);

    expect(wren?.stateValue).not.toBe(STATE_PAIRING);
    expect(kade?.stateValue).not.toBe(STATE_PAIRING);
  });

  test('pairing partner missing or unknown leaves pairedWith empty and records unmeasuredSource', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [P_WREN],
      activePairs: [{ card: 4560, navigator: 'wren', driver: '' }],
      sessions: [
        { id: 's-wren', principal: P_WREN, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: { wren: { lastActionAt: SEEN_1219, recent5mCount: 2 } },
      wipCards: [{ cardId: 4560, owner: 'wren' }],
    };

    const states = deriveActorState(input);
    const wren = states.find((s) => s.aboutPrincipal === P_WREN);

    expect(wren?.stateValue).toBe(STATE_PAIRING);
    expect(wren?.pairRole).toBe(ROLE_NAVIGATOR);
    expect(wren?.pairedWith).toBeUndefined();
    expect(wren?.unmeasuredSource).toEqual([
      { source: 'pair-registry', reason: 'no partner recorded on #4560' },
    ]);
  });
});

describe('Actor State - Team Roles Filtering', () => {
  test('isTeamRole returns true only for agent and human team roles based on roleKind', () => {
    expect(isTeamRole({ name: 'kade', roleKind: ROLE_AGENT })).toBe(true);
    expect(isTeamRole({ name: 'jeff', roleKind: ROLE_HUMAN })).toBe(true);
    expect(isTeamRole({ name: 'crawler', roleKind: ROLE_AUTOMATION })).toBe(false);
    expect(isTeamRole({ name: 'nightly', roleKind: ROLE_AUTOMATION })).toBe(false);
    expect(isTeamRole({ name: 'zz-probe-20260923', roleKind: 'zz-probe-kind' })).toBe(false);
  });
});

describe('Actor State - Available', () => {
  test('alive with live session and no card derives available for agents and humans', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [
        { principal: P_SILAS, roleKind: ROLE_AGENT },
        { principal: P_JEFF, roleKind: ROLE_HUMAN },
      ],
      sessions: [
        { id: 's-silas', principal: P_SILAS, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
        { id: 's-jeff', principal: P_JEFF, channel: CH_BROWSER, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: {
        silas: { lastActionAt: SEEN_1219, recent5mCount: 2 },
        jeff: { lastActionAt: SEEN_1219, recent5mCount: 0 },
      },
      wipCards: [],
      activePairs: [],
      reviewCards: [],
      owedNudges: [],
    };

    const states = deriveActorState(input);
    const silas = states.find((s) => s.aboutPrincipal === P_SILAS);
    const jeff = states.find((s) => s.aboutPrincipal === P_JEFF);

    expect(silas?.stateValue).toBe(STATE_AVAILABLE);
    expect(jeff?.stateValue).toBe(STATE_AVAILABLE);
  });
});

describe('Actor State - Name and Card Title', () => {
  test('actor carries display name and currentCardTitle from board/roles', () => {
    const input: ActorFactInput = {
      now: baseNow,
      principals: [
        { principal: P_KADE, name: 'Kade', roleKind: ROLE_AGENT },
        { principal: P_ABBY, roleKind: ROLE_AGENT },
      ],
      wipCards: [{ cardId: 4560, title: 'Clearing v2 only presents', owner: 'kade' }],
      sessions: [
        { id: 's-kade', principal: P_KADE, channel: CH_PANE, expiresAt: EXP_1300, lastSeenAt: SEEN_1219 },
      ],
      actions: {
        kade: { lastActionAt: SEEN_1219, recent5mCount: 4 },
      },
    };

    const states = deriveActorState(input);
    const kade = states.find((s) => s.aboutPrincipal === P_KADE);
    const abby = states.find((s) => s.aboutPrincipal === P_ABBY);

    expect(kade?.name).toBe('Kade');
    expect(kade?.currentCardTitle).toBe('Clearing v2 only presents');
    expect(abby?.name).toBe('Abby Normal');
  });
});



