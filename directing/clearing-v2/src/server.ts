// @domain: messages

import express, { Request, Response } from 'express';
import fs from 'fs';
import path from 'path';
import { MessageRouter } from './router';
import { TileService } from './tile-service';
import { BuzzRelayClient } from './relay-client';
import { verifyNostrEvent, resolveRoleByPubkey, resolveSenderIdentity } from './identity';
import { decideBind } from './room-bind';
import { shareSessionFromHeader, verifyShareSession, readCookieKey } from './share-session';
import { ClearingMessage, NostrEvent } from './types';

import { renderTodayPage, renderRolesPage, renderRoomsPage } from './views';
import { DataService } from './data-service';

export interface ClearingV2Config {
  buzzRelayUrl?: string;
  pulseUrl?: string;
  rolesUrl?: string;
  boardUrl?: string;
  cardsUrl?: string;
  alertsUrl?: string;
  port?: number;
  host?: string;
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
const PULSE_URL = process.env.PULSE_URL || activeConfig.pulseUrl || 'http://127.0.0.1:3475';
const ROLES_URL = process.env.ROLES_URL || activeConfig.rolesUrl || 'http://localhost:3360/v1/roles/roles';
const CARDS_URL = process.env.CARDS_URL || activeConfig.cardsUrl || 'http://localhost:3360/v1/cards/cards';
const ALERTS_URL = process.env.ALERTS_URL || activeConfig.alertsUrl || 'http://localhost:3340/api/chorus/context/alerts';

// Known pubkeys map for role attribution
const KNOWN_KEYS = new Map<string, string>([
  ['b9d4bfd85efd8aeeacaef985d4e12325cb1d7d527755be875a3813836e06d9b7', 'abby-normal'],
]);

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
  relayUrl: string;
  onInboundEvent: (event: NostrEvent) => void;
  keys: Map<string, string>;
  cookieKey?: Buffer | null;
}

// Helper: test overrides only allowed via x-* headers in test mode, NEVER via URL query parameters
function getTestOverrides(req: Request) {
  const isTest = process.env.NODE_ENV === 'test' || Boolean(req.headers['x-clearing-test']);
  if (!isTest) return {};
  return {
    pulseUrl: req.headers['x-pulse-url'] as string | undefined,
    rolesUrl: req.headers['x-roles-url'] as string | undefined,
    cardsUrl: req.headers['x-cards-url'] as string | undefined,
    alertsUrl: req.headers['x-alerts-url'] as string | undefined,
  };
}

function registerPageRoutes(ctx: RouteContext): void {
  const { app, dataService } = ctx;

  app.get('/', async (req: Request, res: Response) => {
    const { pulseUrl, rolesUrl } = getTestOverrides(req);
    const roomsData = await dataService.getRoomsData('team', { rolesUrl });
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    res.type('html').send(renderRoomsPage('team', roomsData, sources));
  });

  app.get('/today', async (req: Request, res: Response) => {
    const { pulseUrl, alertsUrl } = getTestOverrides(req);
    const [todayData, sources] = await Promise.all([
      dataService.getTodayData({ pulseUrl, alertsUrl }),
      dataService.getTrustStripSources({ pulseUrl }),
    ]);
    res.type('html').send(renderTodayPage(todayData, sources));
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
    const roomsData = await dataService.getRoomsData(room, { rolesUrl });
    const sources = await dataService.getTrustStripSources({ pulseUrl });
    res.type('html').send(renderRoomsPage(room, roomsData, sources));
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
    const { cardsUrl } = getTestOverrides(req);
    const card = await dataService.getCard(cardId, { cardsUrl });
    if (!card) {
      res.status(404).json({ ok: false, error: `card #${cardId} not found` });
      return;
    }
    res.json({ ok: true, card });
  });

  app.get('/api/events', (req: Request, res: Response) => {
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');
    res.flushHeaders();

    ctx.sseClients.push(res);

    res.write(`event: init\ndata: ${JSON.stringify({ messages: router.getRecent(50), tiles: tileService.getTiles() })}\n\n`);

    req.on('close', () => {
      const idx = ctx.sseClients.indexOf(res);
      if (idx !== -1) ctx.sseClients.splice(idx, 1);
    });
  });
}

function registerWriteRoutes(ctx: RouteContext): void {
  const { app, relay, onInboundEvent, keys, cookieKey } = ctx;

  // #3743 — resolveSenderIdentity: identity comes strictly from the verified cryptographic
  // event/session; the client-supplied `from` field and 'jeff' default are RETIRED.
  // (Ported from v1 server.ts:1700-1705 / sender-identity.ts — the exact hole Mark fell through in #3743).
  app.post('/api/send', (req: Request, res: Response) => {
    const event: NostrEvent | undefined = req.body?.event || (req.body?.id && req.body?.sig ? req.body : undefined);

    const identity = resolveSenderIdentity(event, keys);
    if (!identity) {
      res.status(401).json({ ok: false, error: 'unsigned or unregistered post refused: valid signed event from registered key required (#3743)' });
      return;
    }

    relay.publish(event!);
    onInboundEvent(event!);
    res.json({ ok: true, id: event!.id, from: identity.role });
  });

  // #3827 — Room key bind endpoint: identity comes strictly from the verified session cookie,
  // NEVER from a client-supplied header or body field.
  app.post('/api/room/bind', (req: Request, res: Response) => {
    const principal = sessionPrincipal(req, cookieKey);
    const outcome = decideBind(principal?.webid, req.body?.pubkey);
    if (!outcome.ok) {
      res.status(outcome.status).json({ ok: false, error: outcome.error, detail: outcome.detail });
      return;
    }
    keys.set(outcome.pubkey, principal!.name);
    res.json({ ok: true, webid: outcome.webid, pubkey: outcome.pubkey, role: principal!.name });
  });
}

function registerRoutes(ctx: RouteContext): void {
  registerReadApiRoutes(ctx);
  registerPageRoutes(ctx);
  registerWriteRoutes(ctx);
}

export interface CreateAppOptions {
  relayUrl?: string;
  pulseUrl?: string;
  rolesUrl?: string;
  /** Accepted for older callers; unread since #4563 (roles, today and the strip read pulse). */
  boardUrl?: string;
  cardsUrl?: string;
  alertsUrl?: string;
  knownKeys?: Map<string, string>;
  relayConfigured?: boolean;
  cookieKey?: Buffer | null;
}

interface ResolvedUrls {
  pulseUrl: string;
  relayUrl: string;
  rolesUrl: string;
  cardsUrl: string;
  alertsUrl: string;
}

function resolveUrls(options: CreateAppOptions = {}): ResolvedUrls {
  return {
    pulseUrl: options.pulseUrl || PULSE_URL,
    relayUrl: options.relayUrl ?? BUZZ_RELAY_URL,
    rolesUrl: options.rolesUrl || ROLES_URL,
    cardsUrl: options.cardsUrl || CARDS_URL,
    alertsUrl: options.alertsUrl || ALERTS_URL,
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

function toClearingMessage(event: NostrEvent, role: string, tick: boolean): ClearingMessage {
  return {
    id: event.id,
    from: role,
    text: event.content,
    ts: new Date(event.created_at * 1000).toISOString(),
    type: role === 'jeff' ? 'jeff-input' : 'role-response',
    visible: true,
    pubkey: event.pubkey,
    room: roomOf(event),
    tick,
  };
}

interface InboundDeps {
  router: MessageRouter;
  keys: Map<string, string>;
  pulseUrl: string;
  broadcastSSE: (type: string, data: unknown) => void;
  isRelayConnected: () => boolean;
}

/** Handles an incoming verified event from the Buzz relay: ingest, broadcast, forward nudges to Pulse. */
function makeInboundHandler(deps: InboundDeps): (event: NostrEvent) => void {
  return (event: NostrEvent) => {
    if (!verifyNostrEvent(event)) {
      return;
    }
    const role = resolveRoleByPubkey(event.pubkey, deps.keys) || 'unattributed';
    const msg = toClearingMessage(event, role, deps.isRelayConnected());
    if (!deps.router.ingest(msg)) return;
    deps.broadcastSSE('message', msg);
    for (const nudge of deps.router.extractNudges(msg)) {
      fetch(`${deps.pulseUrl}/api/nudge`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(nudge),
      }).catch(() => {
        // fire-and-forget: Pulse delivery failure does not block the room; the message is already ingested
      });
    }
  };
}

export function createApp(options?: CreateAppOptions) {
  const app = express();
  mountStatic(app);

  const router = new MessageRouter(500);
  const urls = resolveUrls(options);
  const tileService = new TileService(urls.pulseUrl);
  const keys = options?.knownKeys || KNOWN_KEYS;
  const relayConfigured = options?.relayConfigured ?? Boolean(urls.relayUrl);

  // Track SSE clients
  const sseClients: Response[] = [];
  const broadcastSSE = makeBroadcaster(sseClients);

  // relay is declared below; the handler only runs once the relay delivers events.
  const onInboundEvent = makeInboundHandler({
    router,
    keys,
    pulseUrl: urls.pulseUrl,
    broadcastSSE,
    isRelayConnected: () => relay.isConnected(),
  });

  const relay = new BuzzRelayClient({
    relayUrl: urls.relayUrl,
    topic: 'team',
    onEvent: onInboundEvent,
  });

  const dataService = new DataService({
    rolesUrl: urls.rolesUrl,
    cardsUrl: urls.cardsUrl,
    alertsUrl: urls.alertsUrl,
    pulseUrl: urls.pulseUrl,
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
    relayUrl: urls.relayUrl,
    onInboundEvent,
    keys,
    cookieKey: options?.cookieKey,
  });

  return { app, router, tileService, dataService, relay };
}

// Start standalone if executed directly
if (require.main === module) {
  const { app, relay } = createApp();
  if (BUZZ_RELAY_URL) {
    relay.connect();
  }

  app.listen(PORT, HOST, () => {
    console.log(`Clearing v2 running at http://${HOST}:${PORT}`);
    if (BUZZ_RELAY_URL) {
      console.log(`Connected to Buzz relay at ${BUZZ_RELAY_URL}`);
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
