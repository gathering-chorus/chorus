// @domain: messages

import express, { Request, Response, NextFunction } from 'express';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { MessageRouter } from './router';
import { TileService } from './tile-service';
import { BuzzRelayClient, RelaySigner } from './relay-client';
import { bridgeSigner } from './relay-signer';
import { stripBase, withBase, injectBase } from './base-path';
import { deliverJeffInput } from './jeff-input';
import { verifyNostrEvent, resolveRoleByPubkey, resolveSenderIdentity } from './identity';
import { RegistryKeys, DEFAULT_KEYS_URL } from './key-registry';
import { decideBind } from './room-bind';
import { shareSessionFromHeader, verifyShareSession, readCookieKey } from './share-session';
import { ClearingMessage, NostrEvent } from './types';

import { renderTodayPage, renderRolesPage, renderRoomsPage, renderBoardPage, renderCountsPage, renderSyncPage, renderRoomMessage, renderStreamsPage, renderPeopleRow } from './views';
import { byDay, DOUBLE_COLUMNS, fetchReadings, syncByDay } from './sources/readings';
import { fetchStreams } from './sources/streams';
import { registerUploadRoutes, DEFAULT_UPLOAD_DIR } from './uploads';
import { registerVoiceRoutes, DEFAULT_VOICE_DIR } from './voice';
import { fetchBoard } from './sources/board';
import { DataService, ROOM_PAGE_MESSAGES } from './data-service';
import { UndeliveredLog, DEFAULT_UNDELIVERED_FILE } from './undelivered';
import { HandoffLog, DEFAULT_HANDOFF_FILE } from './handoff-log';
import { PublishResult } from './jeff-publisher';
import { buildJeffPublisher, JeffSignerConfig } from './jeff-signer-config';
import { UnsignedEvent } from './remote-signer';

export interface ClearingV2Config {
  buzzRelayUrl?: string;
  /** #4571 — the Host the relay serves; the tunnel dial (127.0.0.1:13000) 404s without it. */
  buzzRelayHost?: string;
  /** #4571 — the common door's v2 address, where a session exists (Silas: no second sign-in). */
  signInUrl?: string;
  pulseUrl?: string;
  rolesUrl?: string;
  boardUrl?: string;
  /** #4597 — the spine stream the Streams tab shows (the old Clearing's /api/stream). */
  streamUrl?: string;
  cardsUrl?: string;
  readingsUrl?: string;
  alertsUrl?: string;
  port?: number;
  host?: string;
  /** #4652 — Jeff's signing service; set, his Clearing posts are stored in Buzz as him. */
  jeffSigner?: JeffSignerConfig;
}

export function loadConfig(explicitPath?: string): ClearingV2Config {
  const configPath = explicitPath || process.env.CLEARING_CONFIG;
  if (!configPath) return {};
  const resolvedPath = path.resolve(process.cwd(), configPath);
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- path comes from --config / CLEARING_CONFIG (operator config), not a request
  if (fs.existsSync(resolvedPath)) {
    try {
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- same operator-config path as above
      return JSON.parse(fs.readFileSync(resolvedPath, 'utf8'));
    } catch (e) {
      console.warn(`Failed to parse config from ${resolvedPath}:`, e);
    }
  }
  return {};
}

const configArgIdx = process.argv.indexOf('--config');
const cliConfigPath = configArgIdx !== -1 && process.argv[configArgIdx + 1] ? process.argv[configArgIdx + 1] : undefined;
const activeConfig = loadConfig(cliConfigPath);

const PORT = parseInt(process.env.PORT || (activeConfig.port ? String(activeConfig.port) : '3472'), 10);
const HOST = process.env.HOST || activeConfig.host || '127.0.0.1';
const BUZZ_RELAY_URL = process.env.BUZZ_RELAY_URL || activeConfig.buzzRelayUrl || '';
const BUZZ_RELAY_HOST = process.env.BUZZ_RELAY_HOST_HEADER || activeConfig.buzzRelayHost || '';
const SIGN_IN_URL = process.env.CLEARING_V2_SIGNIN_URL || activeConfig.signInUrl || '';

/** #4571 — the refusal for join/send without a session: why, and where to sign in. */
export function notSignedIn(signInUrl: string | undefined): { ok: false; error: 'not-signed-in'; detail: string; signInUrl?: string } {
  return signInUrl
    ? { ok: false, error: 'not-signed-in', detail: `Not signed in here. Sign in at ${signInUrl} to join the room and send.`, signInUrl }
    : { ok: false, error: 'not-signed-in', detail: 'Not signed in: sign in through the common door to join the room and send.' };
}
const PULSE_URL = process.env.PULSE_URL || activeConfig.pulseUrl || 'http://127.0.0.1:3475';
const ROLES_URL = process.env.ROLES_URL || activeConfig.rolesUrl || 'http://localhost:3360/v1/roles/roles';
const BOARD_URL = process.env.BOARD_URL || activeConfig.boardUrl || 'http://localhost:3340/api/chorus/context/board';
const STREAM_URL = process.env.STREAM_URL || activeConfig.streamUrl || 'http://127.0.0.1:3470/api/stream';
const CARDS_URL = process.env.CARDS_URL || activeConfig.cardsUrl || 'http://localhost:3360/v1/cards/cards';
const ALERTS_URL = process.env.ALERTS_URL || activeConfig.alertsUrl || 'http://localhost:3340/api/chorus/context/alerts';
/** #4634 — the herald sensors' readings, which /counts presents. */
const READINGS_URL = process.env.READINGS_URL || activeConfig.readingsUrl || 'http://localhost:3360/v1/heralds/sensorreadings';
/** #4625 — a demo v2 reads the real room and never writes it: no send, no pulse hand-off. */
const READ_ONLY = process.env.CLEARING_V2_READONLY === '1';
export const READ_ONLY_REASON = 'read-only demo: this Clearing does not send';

/** #4652 — the security graph's key registry: the only source of whose key is whose. */
const KEYS_URL = process.env.CLEARING_V2_KEYS_URL || DEFAULT_KEYS_URL;

export function shareSessionWebId(req: Request, keyBuffer?: Buffer | null): string | null {
  const raw = shareSessionFromHeader(req.headers.cookie);
  if (!raw) return null;
  const key = keyBuffer !== undefined ? keyBuffer : readCookieKey();
  if (!key) return null;
  const v = verifyShareSession(raw, key);
  return v.ok ? v.webid : null;
}

export function sessionPrincipal(req: Request, keyBuffer?: Buffer | null): { id: string; name: string; webid: string } | null {
  const webid = shareSessionWebId(req, keyBuffer);
  if (!webid) return null;
  if (webid === 'https://id.lightlifeurbangardens.com/jeff/profile/card#me' || webid.includes('/jeff/')) {
    return { id: 'principal-jeff', name: 'jeff', webid };
  }
  const m = webid.match(/\/([^/]+)\/profile\/card/);
  const name = m ? m[1] : webid;
  return { id: `principal-${name}`, name, webid };
}

interface RouteContext {
  app: express.Express;
  router: MessageRouter;
  tileService: TileService;
  dataService: DataService;
  relay: BuzzRelayClient;
  sseClients: Response[];
  /** #4649 — Jeff's messages that did not reach a role, sent to every new page. */
  undelivered: UndeliveredLog;
  relayUrl: string;
  onInboundEvent: (event: NostrEvent) => void;
  keys: Map<string, string>;
  cookieKey?: Buffer | null;
  signInUrl?: string;
  boardUrl: string;
  streamUrl: string;
  /** #4592 — the counts page reads pulse's message store. */
  pulseUrl: string;
  /** #4634 — the herald sensors' readings, presented by /counts. */
  readingsUrl: string;
  /** #4625 — refuse /api/send by name. */
  readOnly: boolean;
  /** #4652 — stores Jeff's posts in Buzz as Jeff via his signing service; absent = the #4608 room copy. */
  jeffPublisher?: JeffPostPublisher;
}

/** #4652 — anything that can store a post in Buzz signed as Jeff (JeffPublisher in prod). */
export interface JeffPostPublisher { publish(unsigned: UnsignedEvent): Promise<PublishResult> }

// Helper: test overrides only allowed via x-* headers in test mode, NEVER via URL query parameters
function getTestOverrides(req: Request) {
  const isTest = process.env.NODE_ENV === 'test' || Boolean(req.headers['x-clearing-test']);
  if (!isTest) return {};
  return {
    pulseUrl: req.headers['x-pulse-url'] as string | undefined,
    rolesUrl: req.headers['x-roles-url'] as string | undefined,
    cardsUrl: req.headers['x-cards-url'] as string | undefined,
    athenaCardUrl: req.headers['x-athena-card-url'] as string | undefined,
    alertsUrl: req.headers['x-alerts-url'] as string | undefined,
  };
}

function registerPageRoutes(ctx: RouteContext): void {
  const { app, dataService } = ctx;

  app.get('/', async (req: Request, res: Response) => {
    const { pulseUrl, rolesUrl } = getTestOverrides(req);
    const roomsData = await dataService.getRoomsData('team', { rolesUrl, pulseUrl });
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    res.type('html').send(renderRoomsPage('team', roomsData, sources));
  });

  // #4634 — messages and events by day, from the herald sensors' readings; v2 derives nothing.
  app.get('/counts', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    try {
      const readings = await fetchReadings(ctx.readingsUrl);
      res.type('html').send(renderCountsPage(byDay(readings), {}, sources, byDay(readings, DOUBLE_COLUMNS)));
    } catch (e) {
      res.type('html').send(renderCountsPage([], { error: (e as Error).message }, sources));
    }
  });

  // #4647 — git vs graph by day, from the sync sensor's readings; v2 derives nothing.
  app.get('/sync', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    try {
      res.type('html').send(renderSyncPage(syncByDay(await fetchReadings(ctx.readingsUrl)), {}, sources));
    } catch (e) {
      res.type('html').send(renderSyncPage([], { error: (e as Error).message }, sources));
    }
  });

  app.get('/today', async (req: Request, res: Response) => {
    const { pulseUrl, alertsUrl } = getTestOverrides(req);
    const [todayData, sources] = await Promise.all([
      dataService.getTodayData({ pulseUrl, alertsUrl }),
      dataService.getTrustStripSources({ pulseUrl }),
    ]);
    res.type('html').send(renderTodayPage(todayData, sources));
  });

  // #4574 — the Board tab: the board API's columns as served; ?card=N opens that card.
  app.get('/board', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const card = Number(req.query.card);
    const [columns, sources, rolesData] = await Promise.all([
      fetchBoard(ctx.boardUrl),
      dataService.getTrustStripSources({ pulseUrl }),
      dataService.getRolesData({ pulseUrl }),
    ]);
    res.type('html').send(renderBoardPage(columns, Number.isInteger(card) && card > 0 ? card : null, sources, rolesData));
  });

  // #4597 — the Streams tab: each role's recent commands, refreshed in place.
  app.get('/streams', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const [streams, sources] = await Promise.all([fetchStreams(ctx.streamUrl), dataService.getTrustStripSources({ pulseUrl })]);
    res.type('html').send(renderStreamsPage(streams, sources));
  });

  app.get('/roles', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const [rolesData, sources] = await Promise.all([
      dataService.getRolesData({ pulseUrl }),
      dataService.getTrustStripSources({ pulseUrl }),
    ]);
    res.type('html').send(renderRolesPage(rolesData, sources));
  });

  app.get('/rooms/:room', async (req: Request, res: Response) => {
    const rawRoom = req.params.room;
    const room = (Array.isArray(rawRoom) ? rawRoom[0] : rawRoom) || 'team';
    const { pulseUrl, rolesUrl } = getTestOverrides(req);
    const roomsData = await dataService.getRoomsData(room, { rolesUrl, pulseUrl });
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    res.type('html').send(renderRoomsPage(room, roomsData, sources));
  });

  // #4581 — numeric card shortcut: /1 or /clearing-v2/1 redirects to board with that card open
  app.get('/:card', (req: Request, res: Response, next: NextFunction) => {
    const rawCard = req.params.card;
    const card = Array.isArray(rawCard) ? rawCard[0] : rawCard;
    if (typeof card === 'string' && /^\d+$/.test(card)) {
      const base = res.locals.base || '';
      return res.redirect(`${base}/board?card=${card}`);
    }
    next();
  });
}

function registerReadApiRoutes(ctx: RouteContext): void {
  const { app, router, tileService, dataService, relay, relayUrl } = ctx;

  app.get('/health', (_req: Request, res: Response) => {
    res.json({
      status: 'ok',
      ok: true,
      service: 'clearing-v2',
      version: '2.0.0',
      port: PORT,
      relayConnected: relay.isConnected(),
      relayUrl,
    });
  });

  app.get('/api/trust-strip', async (req: Request, res: Response) => {
    const { pulseUrl } = getTestOverrides(req);
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    res.json(sources);
  });

  app.get('/api/messages', (_req: Request, res: Response) => {
    res.json(router.getRecent(100));
  });

  // #4646 — the role tiles, re-fetched by the page every few seconds so a card that
  // closed stops showing (Jeff 12:53: "it shows 4635 wip and ur message says its done").
  app.get('/api/rooms/:room/people', async (req: Request, res: Response) => {
    const rawRoom = req.params.room;
    const room = (Array.isArray(rawRoom) ? rawRoom[0] : rawRoom) || 'team';
    const { pulseUrl } = getTestOverrides(req);
    const roles = await dataService.getRolesData({ pulseUrl });
    res.json({ html: withBase(renderPeopleRow(room, roles.actors), String(res.locals.base || '')) });
  });

  // #4597 — Jeff 19:11: "i cant scroll up in message history". The page of messages just
  // older than ?before=<id>, rendered as the room renders them, for the room to prepend.
  app.get('/api/rooms/:room/history', (req: Request, res: Response) => {
    const rawRoom = req.params.room;
    const room = (Array.isArray(rawRoom) ? rawRoom[0] : rawRoom) || 'team';
    const before = typeof req.query.before === 'string' ? req.query.before : '';
    const older = before ? dataService.getRoomHistory(room, before) : [];
    // #4646 — the older page carries the same base path as the page (photo and room links)
    res.json({ count: older.length, html: withBase(older.map(renderRoomMessage).join(''), String(res.locals.base || '')) });
  });

  // #4597 — Jeff 16:25: "i really need a streams menu option too". Each role's recent
  // commands, read from the spine stream the old Clearing already serves; this only shows it.
  app.get('/api/streams', async (_req: Request, res: Response) => {
    res.json(await fetchStreams(ctx.streamUrl));
  });

  app.get('/api/tiles', async (_req: Request, res: Response) => {
    const tiles = await tileService.refresh();
    res.json(tiles);
  });

  app.get('/api/cards/:id', async (req: Request, res: Response) => {
    const rawId = req.params.id;
    const idStr = Array.isArray(rawId) ? rawId[0] : rawId;
    const cardId = parseInt(idStr, 10);
    if (isNaN(cardId)) {
      res.status(400).json({ ok: false, error: 'invalid card id' });
      return;
    }
    const { cardsUrl, athenaCardUrl } = getTestOverrides(req);
    const card = await dataService.getCard(cardId, { cardsUrl, athenaCardUrl });
    if (!card) {
      res.status(404).json({ ok: false, error: `card #${cardId} not found` });
      return;
    }
    res.json({ ok: true, card });
  });

  // #4598: Read live /cws pipeline status for a card from chorus-werk-status
  app.get(['/api/cards/:id/pipeline', '/api/cards/:id/cws'], async (req: Request, res: Response) => {
    const rawId = req.params.id;
    const idStr = Array.isArray(rawId) ? rawId[0] : rawId;
    const cardId = parseInt(idStr, 10);
    if (isNaN(cardId)) {
      res.status(400).json({ ok: false, error: 'invalid card id' });
      return;
    }
    const pipeline = await dataService.getCardPipeline(cardId);
    res.json({ ok: true, pipeline });
  });

  app.get('/api/events', (req: Request, res: Response) => {
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');
    res.flushHeaders();

    ctx.sseClients.push(res);

    res.write(`event: init\ndata: ${JSON.stringify({ messages: router.getRecent(ROOM_PAGE_MESSAGES), tiles: tileService.getTiles(), undelivered: ctx.undelivered.all() })}\n\n`);

    req.on('close', () => {
      const idx = ctx.sseClients.indexOf(res);
      if (idx !== -1) ctx.sseClients.splice(idx, 1);
    });
  });
}

/**
 * #4652 — where Jeff's Clearing post is stored in Buzz. With his signing service set up,
 * as Jeff himself, and never as a room copy (a failure is reported, Silas 11:56). Without
 * it, #4608's copy signed by the room and author-tagged. Both carry his signed id, so the
 * echo merges with what the room already shows.
 */
async function storeInBuzz(ctx: RouteContext, event: NostrEvent, role: string): Promise<{ ok: boolean; reason: string }> {
  const room = roomOf(event);
  const tags = [['t', room], ['room', room], ['author', role], ['signed', event.id], ['surface', 'clearing'], ['kind', 'jeff']];
  if (ctx.jeffPublisher && role === 'jeff') {
    const r = await ctx.jeffPublisher.publish({ kind: 9, created_at: event.created_at, tags: [['h', ctx.relay.getChannel()], ...tags], content: event.content });
    return r.ok ? { ok: true, reason: '' } : { ok: false, reason: r.reason };
  }
  const copy = ctx.relay.signAsReader(event.content, tags, event.created_at);
  return copy ? ctx.relay.publishConfirmed(copy) : { ok: false, reason: 'no reader key' };
}

function registerWriteRoutes(ctx: RouteContext): void {
  const { app, onInboundEvent, keys, cookieKey } = ctx;

  // #3743 — resolveSenderIdentity: identity comes strictly from the verified cryptographic
  // event/session; the client-supplied `from` field and 'jeff' default are RETIRED.
  // (Ported from v1 server.ts:1700-1705 / sender-identity.ts — the exact hole Mark fell through in #3743).
  app.post('/api/send', async (req: Request, res: Response) => {
    if (ctx.readOnly) {
      res.status(403).json({ ok: false, reason: READ_ONLY_REASON });
      return;
    }
    const event: NostrEvent | undefined = req.body?.event || (req.body?.id && req.body?.sig ? req.body : undefined);

    const identity = resolveSenderIdentity(event, keys);
    if (!identity) {
      // #3743: identity only from a verified event whose key a signed-in session bound.
      res.status(401).json(notSignedIn(ctx.signInUrl));
      return;
    }

    // #4608 — the relay stores only notes signed by this connection's identity
    // (the bridge), so the room stores Jeff's words as the bridge, author-tagged,
    // with his own signed id so the copy is never shown twice. Delivery to the
    // roles does not wait on Buzz; the answer says whether Buzz stored it.
    onInboundEvent(event!);
    const stored = await storeInBuzz(ctx, event!, identity.role);
    if (!stored.ok) console.error(`[clearing-v2] ${identity.role}'s message ${event!.id.slice(0, 12)} was not stored in Buzz: ${stored.reason}`);
    res.json({ ok: true, id: event!.id, from: identity.role, stored: stored.ok, ...(stored.ok ? {} : { reason: stored.reason }) });
  });

  // #3827 — Room key bind endpoint: identity comes strictly from the verified session cookie,
  // NEVER from a client-supplied header or body field.
  app.post('/api/room/bind', (req: Request, res: Response) => {
    const principal = sessionPrincipal(req, cookieKey);
    const outcome = decideBind(principal?.webid, req.body?.pubkey);
    if (!outcome.ok) {
      if (outcome.error === 'not-signed-in') { res.status(401).json(notSignedIn(ctx.signInUrl)); return; }
      res.status(outcome.status).json({ ok: false, error: outcome.error, detail: outcome.detail });
      return;
    }
    keys.set(outcome.pubkey, principal!.name);
    res.json({ ok: true, webid: outcome.webid, pubkey: outcome.pubkey, role: principal!.name });
  });
  // #4635 — photos from Jeff's phone; posting and viewing both need a signed-in session.
  registerUploadRoutes(app, {
    dir: DEFAULT_UPLOAD_DIR,
    readOnly: ctx.readOnly,
    readOnlyReason: READ_ONLY_REASON,
    signedIn: (req) => sessionPrincipal(req, cookieKey)?.name || null,
    notSignedIn: () => notSignedIn(ctx.signInUrl),
  });
  registerVoiceRoutes(app, {
    dir: DEFAULT_VOICE_DIR,
    readOnly: ctx.readOnly,
    readOnlyReason: READ_ONLY_REASON,
    signedIn: (req) => sessionPrincipal(req, cookieKey)?.name || null,
    notSignedIn: () => notSignedIn(ctx.signInUrl),
  });
}

function registerRoutes(ctx: RouteContext): void {
  registerReadApiRoutes(ctx);
  registerPageRoutes(ctx);
  registerWriteRoutes(ctx);
}

/** #4649 — where failed hand-offs persist. Prod gets ~/.chorus by default, so a
 *  restart keeps the marks; under jest nothing touches it unless a test passes a path. */
export function resolveUndeliveredFile(explicit: string | undefined, env: NodeJS.ProcessEnv): string | undefined {
  if (explicit) return explicit;
  if (env.JEST_WORKER_ID) return undefined;
  return env.CLEARING_V2_UNDELIVERED_FILE || DEFAULT_UNDELIVERED_FILE;
}

interface KnownKeys { keys: Map<string, string>; ready: Promise<unknown>; stop: () => void }

/**
 * #4652 — the given keys, or an empty map the registry fills. `ready` settles after
 * the first registry read, whether or not it answered, so the relay is dialled only
 * then: the relay's replay arriving first labelled 93 of 100 demo posts unattributed.
 */
function knownKeysFor(options: CreateAppOptions | undefined): KnownKeys {
  if (options?.knownKeys) return { keys: options.knownKeys, ready: Promise.resolve(), stop: () => undefined };
  const url = options?.keysUrl;
  const keys = new Map<string, string>();
  // under jest with no registry given, read nothing (tests never call the live door)
  if (!url && process.env.JEST_WORKER_ID) return { keys, ready: Promise.resolve(), stop: () => undefined };
  const registry = new RegistryKeys(keys, url ?? KEYS_URL);
  return { keys, ready: registry.startWithRetries(3, options?.keysRetryGapMs), stop: () => registry.stop() };
}

/** #4652 — the given publisher, or one from the config's jeffSigner (never under jest). */
function jeffPublisherFor(options: CreateAppOptions | undefined, relayUrl: string): JeffPostPublisher | undefined {
  if (options?.jeffPublisher) return options.jeffPublisher;
  if (process.env.JEST_WORKER_ID) return undefined;
  return buildJeffPublisher(activeConfig.jeffSigner, relayUrl, options?.relayHost ?? (BUZZ_RELAY_HOST || undefined));
}

function makeHandoffLog(options: CreateAppOptions | undefined): HandoffLog | undefined {
  const file = resolveHandoffFile(options?.handoffFile, process.env);
  return file ? new HandoffLog(file) : undefined;
}

/** #4652 — where the hand-off log lives; under jest with no path, nowhere (no log). */
export function resolveHandoffFile(explicit: string | undefined, env: NodeJS.ProcessEnv): string | undefined {
  if (explicit) return explicit;
  if (env.JEST_WORKER_ID) return undefined;
  return env.CLEARING_V2_HANDOFF_FILE || DEFAULT_HANDOFF_FILE;
}

export interface CreateAppOptions {
  /** #4649 — where failed hand-offs persist; tests pass a temp path (none by default under jest). */
  undeliveredFile?: string;
  /** #4652 — the hand-off log's file; tests pass one to exercise it. */
  handoffFile?: string;
  relayUrl?: string;
  pulseUrl?: string;
  rolesUrl?: string;
  /** Accepted for older callers; unread since #4563 (roles, today and the strip read pulse). */
  boardUrl?: string;
  cardsUrl?: string;
  athenaCardUrl?: string;
  alertsUrl?: string;
  runWerkStatus?: (cardId: number) => Promise<string>;
  knownKeys?: Map<string, string>;
  /** #4652 — stores Jeff's Clearing posts as Jeff; absent = the room copy (#4608). */
  jeffPublisher?: JeffPostPublisher;
  /** #4652 — the key registry to read when no knownKeys are given. */
  keysUrl?: string;
  /** #4652 — the wait between start-up reads of the registry (default 5 s). */
  keysRetryGapMs?: number;
  relayConfigured?: boolean;
  cookieKey?: Buffer | null;
  /** #4571 — test seams: the relay's Host and reader key (null = no key), the sign-in door. */
  relayHost?: string;
  signInUrl?: string;
  relaySigner?: RelaySigner | null;
  /** #4634 — test seam: where /counts reads the sensors' readings. */
  readingsUrl?: string;
  /** #4625 — defaults to CLEARING_V2_READONLY=1 (set by the demo env). */
  readOnly?: boolean;
}

interface ResolvedUrls {
  readingsUrl: string;
  pulseUrl: string;
  relayUrl: string;
  rolesUrl: string;
  cardsUrl: string;
  athenaCardUrl?: string;
  alertsUrl: string;
}

function resolveUrls(options: CreateAppOptions = {}): ResolvedUrls {
  return {
    pulseUrl: options.pulseUrl || PULSE_URL,
    relayUrl: options.relayUrl ?? BUZZ_RELAY_URL,
    rolesUrl: options.rolesUrl || ROLES_URL,
    cardsUrl: options.cardsUrl || CARDS_URL,
    athenaCardUrl: options.athenaCardUrl,
    alertsUrl: options.alertsUrl || ALERTS_URL,
    readingsUrl: options.readingsUrl || READINGS_URL,
  };
}

function mountStatic(app: express.Express): void {
  app.use(express.json());
  app.use(express.static(path.join(__dirname, '../public'), { index: false }));
  app.use('/vendor/@noble/curves', express.static(path.join(__dirname, '../node_modules/@noble/curves/esm'), { extensions: ['js'] }));
  app.use('/vendor/@noble/hashes', express.static(path.join(__dirname, '../node_modules/@noble/hashes/esm'), { extensions: ['js'] }));
}

/** Writes an SSE event to every client; a client whose write throws is dropped. */
function makeBroadcaster(sseClients: Response[]): (type: string, data: unknown) => void {
  return (type: string, data: unknown) => {
    const payload = `event: ${type}\ndata: ${JSON.stringify(data)}\n\n`;
    for (let i = sseClients.length - 1; i >= 0; i--) {
      const client = sseClients.at(i);
      if (!client) continue;
      try {
        client.write(payload);
      } catch {
        sseClients.splice(i, 1);
      }
    }
  };
}

function roomOf(event: NostrEvent): string {
  // tags is typed required, but relay events are external input: guard at runtime.
  const tags: unknown = event.tags;
  const roomTag = Array.isArray(tags)
    ? event.tags.find((t) => t[0] === 'room' || (t[0] === 't' && t[1] !== 'reply'))
    : undefined;
  return roomTag ? roomTag[1] : 'team';
}

/** #4592 — the note's kind tag (v1's buildNote), only the two kinds the room's toggle owns. */
export function kindOf(event: { tags?: string[][] }): 'nudge' | 'alert' | undefined {
  const v = (event.tags || []).find((t) => t[0] === 'kind')?.[1];
  return v === 'nudge' || v === 'alert' ? v : undefined;
}

function toClearingMessage(event: NostrEvent, role: string, tick: boolean, signer?: string): ClearingMessage {
  const kind = kindOf(event);
  // #4608 — the room's stored copy of a sender's note carries that note's id, so
  // the copy and the original are one message (the router drops a seen id).
  const signed = Array.isArray(event.tags) ? event.tags.find((t) => t[0] === 'signed')?.[1] : undefined;
  return {
    ...(kind ? { kind } : {}),
    id: signed || event.id,
    from: role,
    text: event.content,
    ts: new Date(event.created_at * 1000).toISOString(),
    type: role === 'jeff' ? 'jeff-input' : 'role-response',
    visible: true,
    pubkey: event.pubkey,
    room: roomOf(event),
    tick,
    signer,
  };
}

interface InboundDeps {
  router: MessageRouter;
  keys: Map<string, string>;
  pulseUrl: string;
  broadcastSSE: (type: string, data: unknown) => void;
  /** #4649 — failed hand-offs, kept so a reload still shows "not delivered". */
  undelivered?: UndeliveredLog;
  isRelayConnected: () => boolean;
  /** #4625 — a demo shows Jeff's notes and never hands them to pulse again. */
  readOnly?: boolean;
  /** #4652 — Jeff's Buzz app posts come back on every relay replay: hand each off once. */
  handoffs?: HandoffLog;
}

/** Handles an incoming verified event from the Buzz relay: ingest, broadcast, forward nudges to Pulse. */
export function makeInboundHandler(deps: InboundDeps): (event: NostrEvent) => void {
  return (event: NostrEvent) => {
    if (!verifyNostrEvent(event)) {
      return;
    }
    const signerRole = resolveRoleByPubkey(event.pubkey, deps.keys) || 'unattributed';
    let role = signerRole;
    // #4591 — on bridge-signed events, unwrap the true author from the author tag
    if (signerRole === 'bridge' && Array.isArray(event.tags)) {
      const authorTag = event.tags.find((t) => t[0] === 'author');
      if (authorTag && authorTag[1]) {
        role = authorTag[1];
      }
    }
    const msg = toClearingMessage(event, role, deps.isRelayConnected(), signerRole);
    if (!deps.router.ingest(msg)) return;
    deps.broadcastSSE('message', msg);
    // #4571/#4591 — only direct cryptographic sends from Jeff are Jeff input;
    // bridge-signed notes are transcript/room mirrors, never a second pulse injection.
    if (signerRole !== 'jeff' || deps.readOnly) return;
    // #4652 — a post Jeff typed in Clearing was handed off at /api/send; its copy in
    // Buzz, signed as Jeff, comes back here and must not reach the roles twice.
    if (Array.isArray(event.tags) && event.tags.some((t) => t[0] === 'surface' && t[1] === 'clearing')) return;
    if (deps.handoffs && !deps.handoffs.claim(event.id, event.created_at)) return;
    for (const nudge of deps.router.extractNudges(msg)) {
      void deliverJeffInput(deps.pulseUrl, nudge.to, nudge.content).then((r) => {
        if (!r.ok) {
          console.error(`[clearing-v2] Jeff's message to ${nudge.to} was not handed off: ${r.reason}`);
          const roles = deps.undelivered ? deps.undelivered.record(msg.id, nudge.to) : [nudge.to];
          deps.broadcastSSE('delivery', { to: nudge.to, ok: false, reason: r.reason, id: msg.id, roles });
        }
      });
    }
  };
}

/** #4571 — read #team as the registered bridge identity, through the relay's own Host. */
function makeRelay(relayUrl: string, onEvent: (event: NostrEvent) => void, options: CreateAppOptions = {}): BuzzRelayClient {
  const reader = options.relaySigner !== undefined ? { signer: options.relaySigner ?? undefined, reason: 'no reader key (test)' } : bridgeSigner(os.homedir());
  if (relayUrl && !reader.signer) console.warn(`Buzz relay: ${reader.reason ?? 'no reader key'} — Rooms cannot read`);
  return new BuzzRelayClient({
    relayUrl,
    topic: 'team',
    onEvent,
    signer: reader.signer,
    hostHeader: options.relayHost ?? (BUZZ_RELAY_HOST || undefined),
  });
}

/** #4571 — strip /clearing-v2 before routing; HTML gets the prefix on its links and as CLEARING_BASE. */
function mountBasePath(app: express.Express): void {
  app.use((req: Request, res: Response, next) => {
    const { url, base } = stripBase(req.url);
    req.url = url;
    res.locals.base = base;
    const send = res.send.bind(res);
    res.send = ((body?: unknown) => send(typeof body === 'string' && /^\s*</.test(body) ? injectBase(withBase(body, base), base) : body)) as typeof res.send;
    next();
  });
}

export function createApp(options?: CreateAppOptions) {
  const app = express();
  mountBasePath(app);
  mountStatic(app);

  const router = new MessageRouter(500);
  const urls = resolveUrls(options);
  const tileService = new TileService(urls.pulseUrl);
  const known = knownKeysFor(options);
  const keys = known.keys;
  const relayConfigured = options?.relayConfigured ?? Boolean(urls.relayUrl);
  const readOnly = options?.readOnly ?? READ_ONLY;

  // Track SSE clients
  const sseClients: Response[] = [];
  const broadcastSSE = makeBroadcaster(sseClients);
  const undelivered = new UndeliveredLog(200, resolveUndeliveredFile(options?.undeliveredFile, process.env));
  const handoffs = makeHandoffLog(options);

  // relay is declared below; the handler only runs once the relay delivers events.
  const onInboundEvent = makeInboundHandler({
    router,
    keys,
    pulseUrl: urls.pulseUrl,
    broadcastSSE,
    undelivered,
    isRelayConnected: () => relay.isConnected(),
    readOnly,
    handoffs,
  });

  const relay = makeRelay(urls.relayUrl, onInboundEvent, options);

  const dataService = new DataService({
    rolesUrl: urls.rolesUrl,
    cardsUrl: urls.cardsUrl,
    athenaCardUrl: urls.athenaCardUrl,
    alertsUrl: urls.alertsUrl,
    pulseUrl: urls.pulseUrl,
    runWerkStatus: options?.runWerkStatus,
    relay,
    router,
    relayConfigured,
  });

  registerRoutes({
    app,
    router,
    tileService,
    dataService,
    relay,
    sseClients,
    undelivered,
    relayUrl: urls.relayUrl,
    onInboundEvent,
    keys,
    cookieKey: options?.cookieKey,
    signInUrl: options?.signInUrl ?? (SIGN_IN_URL || undefined),
    boardUrl: options?.boardUrl ?? BOARD_URL,
    streamUrl: STREAM_URL,
    pulseUrl: urls.pulseUrl,
    readingsUrl: urls.readingsUrl,
    readOnly,
    jeffPublisher: jeffPublisherFor(options, urls.relayUrl),
  });

  return { app, router, tileService, dataService, relay, keys, keysReady: known.ready, stopKeys: known.stop };
}

// Start standalone if executed directly
if (require.main === module) {
  const { app, relay, keysReady } = createApp();
  if (BUZZ_RELAY_URL) {
    void keysReady.then(() => relay.connect());
  }

  app.listen(PORT, HOST, () => {
    console.log(`Clearing v2 running at http://${HOST}:${PORT}`);
    if (BUZZ_RELAY_URL) {
      console.log(`Dialing Buzz relay at ${BUZZ_RELAY_URL}${BUZZ_RELAY_HOST ? ` (Host ${BUZZ_RELAY_HOST})` : ''}; /health says when the room can read`);
    } else {
      console.log('Buzz relay not configured (trust strip shows rooms failed: relay not configured)');
    }

    try {
      const { serviceLifecycle } = require('../../../platform/chorus-sdk/lifecycle/service-lifecycle');
      const lifecycle = serviceLifecycle('com.chorus.clearing-v2');
      process.on('SIGTERM', () => { lifecycle.stopped('SIGTERM'); process.exit(0); });
      process.on('SIGINT', () => { lifecycle.stopped('SIGINT'); process.exit(0); });
      process.on('uncaughtException', (e: Error) => { lifecycle.failed(`uncaughtException: ${e.message}`, 1); process.exit(1); });
      lifecycle.started();
    } catch (e) {
      console.warn('serviceLifecycle unavailable:', e);
    }
  });
}
