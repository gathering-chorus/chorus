// @domain: awareness
// @test-type: unit — pure derivation over fact fixtures, endpoint over express app
// @card: #4563
import request from 'supertest';
import {
  deriveChangesOvernight,
  ChangesOvernightService,
  type ChangesFactInput,
  type RawSessionRow,
  type RawCardMove,
} from './changes-overnight';
import { createApp } from './service';
import { MessageStore } from './store';

const baseNow = new Date('2026-10-16T11:20:00Z');
const WATERMARK_ISO = '2026-10-15T23:00:00Z';
const AFTER_WATERMARK_ISO = '2026-10-16T08:30:00Z';
const BEFORE_WATERMARK_ISO = '2026-10-15T21:00:00Z';
const SRC_SESSIONS = 'athena:identity:sessions';
const SRC_LOKI = 'loki:moves';
const ERR_502 = 'HTTP 502 Bad Gateway';
const CH_BROWSER = 'browser';
const EV_MOVED = 'card.item.moved';
const EV_ACCEPTED = 'card.accepted';
const P_JEFF = 'principal-jeff';
const TITLE_ACTOR_STATE = 'Actor state in pulse';

describe('Changes Overnight — Watermark Derivation', () => {
  test('watermark is Jeff’s most recent closed browser session end', () => {
    const sessions: RawSessionRow[] = [
      {
        name: 's-jeff-1',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'closed',
        endedAt: '2026-10-15T21:00:00Z',
      },
      {
        name: 's-jeff-2',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'closed',
        endedAt: WATERMARK_ISO,
      },
      {
        name: 's-jeff-open',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'open',
      },
      {
        name: 's-kade-pane',
        ownedBy: 'principal-kade',
        channel: 'pane',
        sessionState: 'closed',
        endedAt: '2026-10-16T01:00:00Z',
      },
    ];

    const input: ChangesFactInput = {
      now: baseNow,
      sessions,
      lokiMoves: [],
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBe(WATERMARK_ISO);
    expect(res.unmeasured).toEqual([]);
  });

  test('watermark derives from open-but-expired session before current live session', () => {
    const sessions: RawSessionRow[] = [
      {
        name: 'jeff-browser-old-expired',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'open',
        issuedAt: '2026-10-15T16:00:00Z',
        lastSeenAt: '2026-10-15T22:10:00Z',
      },
      {
        name: 'jeff-browser-live',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'open',
        issuedAt: '2026-10-16T06:30:00Z',
        lastSeenAt: '2026-10-16T07:00:00Z',
      },
    ];

    const input: ChangesFactInput = {
      now: baseNow,
      sessions,
      lokiMoves: [],
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBe('2026-10-15T22:10:00Z');
  });
});

describe('Changes Overnight — Watermark Negative Proofs', () => {
  test('negative proof: unreadable sessions door leaves watermark null and names athena:identity:sessions, never silent 24h', () => {
    const input: ChangesFactInput = {
      now: baseNow,
      sessions: undefined,
      unmeasured: [{ source: SRC_SESSIONS, reason: ERR_502 }],
      lokiMoves: [],
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBeNull();
    expect(res.cards).toEqual([]);
    expect(res.unmeasured.some((u) => u.source === SRC_SESSIONS)).toBe(true);
  });

  test('negative proof: no closed browser session for Jeff leaves watermark null with honest reason', () => {
    const input: ChangesFactInput = {
      now: baseNow,
      sessions: [
        {
          name: 's-kade',
          ownedBy: 'principal-kade',
          channel: 'pane',
          sessionState: 'closed',
          endedAt: '2026-10-16T01:00:00Z',
        },
      ],
      lokiMoves: [],
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBeNull();
    expect(res.cards).toEqual([]);
    expect(res.unmeasured).toEqual([
      { source: SRC_SESSIONS, reason: 'no closed browser session on record' },
    ]);
  });
});

describe('Changes Overnight — Card Filtering', () => {
  test('includes cards moved strictly after watermark, excluding prior moves', () => {
    const sessions: RawSessionRow[] = [
      {
        name: 's-jeff',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'closed',
        endedAt: WATERMARK_ISO,
      },
    ];

    const lokiMoves: RawCardMove[] = [
      {
        event: EV_MOVED,
        card_id: 4560,
        title: TITLE_ACTOR_STATE,
        to: 'Done',
        role: 'wren',
        timestamp: AFTER_WATERMARK_ISO,
      },
      {
        event: EV_MOVED,
        card_id: 4550,
        title: 'Older card',
        to: 'Done',
        role: 'kade',
        timestamp: BEFORE_WATERMARK_ISO,
      },
    ];

    const input: ChangesFactInput = {
      now: baseNow,
      sessions,
      lokiMoves,
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBe(WATERMARK_ISO);
    expect(res.cards.length).toBe(1);
    expect(res.cards[0]).toEqual({
      id: 4560,
      title: TITLE_ACTOR_STATE,
      status: 'Done',
      owner: 'Wren',
      movedAt: AFTER_WATERMARK_ISO,
    });
  });
});

describe('Changes Overnight — Card Deduplication & Errors', () => {
  test('deduplicates multiple moves for same card keeping newest', () => {
    const sessions: RawSessionRow[] = [
      {
        name: 's-jeff',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'closed',
        endedAt: WATERMARK_ISO,
      },
    ];

    const lokiMoves: RawCardMove[] = [
      {
        event: EV_MOVED,
        card_id: 4560,
        title: TITLE_ACTOR_STATE,
        to: 'WIP',
        role: 'wren',
        timestamp: '2026-10-16T08:00:00Z',
      },
      {
        event: EV_ACCEPTED,
        card_id: 4560,
        title: TITLE_ACTOR_STATE,
        role: 'wren',
        timestamp: '2026-10-16T09:30:00Z',
      },
    ];

    const input: ChangesFactInput = {
      now: baseNow,
      sessions,
      lokiMoves,
    };

    const res = deriveChangesOvernight(input);
    expect(res.cards.length).toBe(1);
    expect(res.cards[0].status).toBe('Done');
    expect(res.cards[0].movedAt).toBe('2026-10-16T09:30:00Z');
  });

  test('negative proof: loki moves unreadable names loki source, returns empty cards with single envelope', () => {
    const sessions: RawSessionRow[] = [
      {
        name: 's-jeff',
        ownedBy: P_JEFF,
        channel: CH_BROWSER,
        sessionState: 'closed',
        endedAt: WATERMARK_ISO,
      },
    ];

    const input: ChangesFactInput = {
      now: baseNow,
      sessions,
      lokiMoves: undefined,
      unmeasured: [{ source: SRC_LOKI, reason: 'connection refused' }],
    };

    const res = deriveChangesOvernight(input);
    expect(res.watermark).toBe(WATERMARK_ISO);
    expect(res.cards).toEqual([]);
    expect(res.unmeasured.some((u) => u.source === SRC_LOKI)).toBe(true);
  });
});

describe('Changes Overnight — Service and Endpoint', () => {
  test('ChangesOvernightService caches derived changes and serves on 15s timer', async () => {
    const mockFetcher = jest.fn().mockResolvedValue({
      now: baseNow,
      sessions: [
        {
          name: 's-jeff',
          ownedBy: P_JEFF,
          channel: CH_BROWSER,
          sessionState: 'closed',
          endedAt: WATERMARK_ISO,
        },
      ],
      lokiMoves: [
        {
          event: EV_ACCEPTED,
          card_id: 4560,
          title: TITLE_ACTOR_STATE,
          role: 'wren',
          timestamp: AFTER_WATERMARK_ISO,
        },
      ],
    });

    const service = new ChangesOvernightService(mockFetcher, 15000);
    expect(service.getChanges().cards).toEqual([]);

    await service.refresh();
    const cached = service.getChanges();
    expect(cached.watermark).toBe(WATERMARK_ISO);
    expect(cached.cards.length).toBe(1);
    expect(cached.cards[0].id).toBe(4560);
    expect(cached.unmeasured).toEqual([]);

    service.stop();
  });

  test('GET /api/changes-overnight returns cached changes with single envelope', async () => {
    const store = new MessageStore(':memory:');
    const mockService = {
      getChanges: () => ({
        derivedAt: baseNow.toISOString(),
        watermark: WATERMARK_ISO,
        cards: [
          {
            id: 4560,
            title: TITLE_ACTOR_STATE,
            status: 'Done',
            owner: 'Wren',
            movedAt: AFTER_WATERMARK_ISO,
          },
        ],
        unmeasured: [],
      }),
    };

    const app = createApp(store, undefined, undefined, undefined, mockService as unknown as ChangesOvernightService);
    const res = await request(app).get('/api/changes-overnight');

    expect(res.status).toBe(200);
    expect(typeof res.body.derivedAt).toBe('string');
    expect(res.body.watermark).toBe(WATERMARK_ISO);
    expect(Array.isArray(res.body.cards)).toBe(true);
    expect(res.body.cards.length).toBe(1);
    expect(res.body.cards[0].id).toBe(4560);
    expect(Array.isArray(res.body.unmeasured)).toBe(true);
    expect(res.body.unmeasured).toEqual([]);
  });
});
