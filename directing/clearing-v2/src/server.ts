// @domain: messages

import express, { Request, Response } from 'express';
import fs from 'fs';
import path from 'path';
import { MessageRouter } from './router';
import { TileService } from './tile-service';
import { BuzzRelayClient } from './relay-client';
import { verifyNostrEvent, resolveRoleByPubkey } from './identity';
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
  sessionsUrl?: string;
  lokiUrl?: string;
  port?: number;
  host?: string;
}

export function loadConfig(explicitPath?: string): ClearingV2Config {
  const configPath = explicitPath || process.env.CLEARING_CONFIG;
  if (!configPath) return {};
  const resolvedPath = path.resolve(process.cwd(), configPath);
  if (fs.existsSync(resolvedPath)) {
    try {
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
const BOARD_URL = process.env.BOARD_URL || activeConfig.boardUrl || 'http://localhost:3340/api/chorus/context/board';
const CARDS_URL = process.env.CARDS_URL || activeConfig.cardsUrl || 'http://localhost:3360/v1/cards/cards';
const ALERTS_URL = process.env.ALERTS_URL || activeConfig.alertsUrl || 'http://localhost:3340/api/chorus/context/alerts';
const SESSIONS_URL = process.env.SESSIONS_URL || activeConfig.sessionsUrl || 'http://localhost:3360/v1/identity/sessions';
const LOKI_URL = process.env.LOKI_URL || activeConfig.lokiUrl || 'http://localhost:3102';

// Known pubkeys map for role attribution
const KNOWN_KEYS = new Map<string, string>([
  ['b9d4bfd85efd8aeeacaef985d4e12325cb1d7d527755be875a3813836e06d9b7', 'abby-normal'],
]);

interface RouteContext {
  app: express.Express;
  router: MessageRouter;
  tileService: TileService;
  dataService: DataService;
  relay: BuzzRelayClient;
  sseClients: Response[];
  relayUrl: string;
  pulseUrl: string;
  broadcastSSE: (type: string, data: unknown) => void;
  onInboundEvent: (event: NostrEvent) => void;
  keys: Map<string, string>;
}

function registerRoutes(ctx: RouteContext): void {
  const { app, router, tileService, dataService, relay, sseClients, relayUrl, pulseUrl, broadcastSSE, onInboundEvent, keys } = ctx;

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

  // Helper: test overrides only allowed via x-* headers in test mode, NEVER via URL query parameters
  const getTestOverrides = (req: Request) => {
    const isTest = process.env.NODE_ENV === 'test' || Boolean(req.headers['x-clearing-test']);
    if (!isTest) return {};
    return {
      fixture: req.headers['x-fixture'] as string | undefined,
      failSource: req.headers['x-fail-source'] as string | undefined,
      lagSource: req.headers['x-lag-source'] as string | undefined,
      rolesUrl: req.headers['x-roles-url'] as string | undefined,
      boardUrl: req.headers['x-board-url'] as string | undefined,
      cardsUrl: req.headers['x-cards-url'] as string | undefined,
      alertsUrl: req.headers['x-alerts-url'] as string | undefined,
      lokiUrl: req.headers['x-loki-url'] as string | undefined,
      sessionsUrl: req.headers['x-sessions-url'] as string | undefined,
    };
  };

  app.get('/', async (req: Request, res: Response) => {
    const { fixture, failSource, lagSource, rolesUrl } = getTestOverrides(req);
    const roomsData = await dataService.getRoomsData('team', { rolesUrl });
    const sources = await dataService.getTrustStripSources({ fixture, failSource, lagSource, rolesUrl });
    res.type('html').send(renderRoomsPage('team', roomsData, sources));
  });

  app.get('/today', async (req: Request, res: Response) => {
    const { fixture, failSource, lagSource, rolesUrl, boardUrl, cardsUrl, alertsUrl, sessionsUrl, lokiUrl } = getTestOverrides(req);
    const todayData = await dataService.getTodayData({ fixture, rolesUrl, boardUrl, cardsUrl, alertsUrl, sessionsUrl, lokiUrl });
    const sources = await dataService.getTrustStripSources({ fixture, failSource, lagSource, rolesUrl, boardUrl, lokiUrl });
    res.type('html').send(renderTodayPage(todayData, sources));
  });

  app.get('/roles', async (req: Request, res: Response) => {
    const { fixture, failSource, lagSource, rolesUrl, boardUrl, lokiUrl, sessionsUrl } = getTestOverrides(req);
    const rolesData = await dataService.getRolesData({ rolesUrl, boardUrl, lokiUrl, sessionsUrl, fixture });
    const sources = await dataService.getTrustStripSources({ rolesUrl, boardUrl, failSource, lagSource, fixture });
    res.type('html').send(renderRolesPage(rolesData, sources));
  });

  app.get('/rooms/:room', async (req: Request, res: Response) => {
    const rawRoom = req.params.room;
    const room = (Array.isArray(rawRoom) ? rawRoom[0] : rawRoom) || 'team';
    const { fixture, failSource, lagSource, rolesUrl } = getTestOverrides(req);
    const roomsData = await dataService.getRoomsData(room, { rolesUrl });
    const sources = await dataService.getTrustStripSources({ failSource, lagSource, fixture, rolesUrl });
    res.type('html').send(renderRoomsPage(room, roomsData, sources));
  });

  app.get('/api/trust-strip', async (req: Request, res: Response) => {
    const { fixture, failSource, lagSource, rolesUrl, boardUrl } = getTestOverrides(req);
    const sources = await dataService.getTrustStripSources({ rolesUrl, boardUrl, failSource, lagSource, fixture });
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
    const { boardUrl, cardsUrl } = getTestOverrides(req);
    const card = await dataService.getCard(cardId, { boardUrl, cardsUrl });
    if (!card) {
      res.status(404).json({ ok: false, error: `card #${cardId} not found` });
      return;
    }
    res.json({ ok: true, card });
  });

  app.post('/api/send', (req: Request, res: Response) => {
    const event: NostrEvent | undefined = req.body?.event || (req.body?.id && req.body?.sig ? req.body : undefined);

    if (!event || !verifyNostrEvent(event)) {
      res.status(401).json({ ok: false, error: 'unsigned post refused: valid signed event required' });
      return;
    }

    const role = resolveRoleByPubkey(event.pubkey, keys);
    if (!role) {
      res.status(401).json({ ok: false, error: 'unregistered key refused' });
      return;
    }

    relay.publish(event);
    onInboundEvent(event);
    res.json({ ok: true, id: event.id, from: role });
  });


  app.get('/api/events', (req: Request, res: Response) => {
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');
    res.flushHeaders();

    sseClients.push(res);

    res.write(`event: init\ndata: ${JSON.stringify({ messages: router.getRecent(50), tiles: tileService.getTiles() })}\n\n`);

    req.on('close', () => {
      const idx = sseClients.indexOf(res);
      if (idx !== -1) sseClients.splice(idx, 1);
    });
  });
}

export function createApp(options?: {
  relayUrl?: string;
  pulseUrl?: string;
  rolesUrl?: string;
  boardUrl?: string;
  cardsUrl?: string;
  alertsUrl?: string;
  sessionsUrl?: string;
  lokiUrl?: string;
  knownKeys?: Map<string, string>;
  relayConfigured?: boolean;
}) {
  const app = express();
  app.use(express.json());
  app.use(express.static(path.join(__dirname, '../public'), { index: false }));

  const router = new MessageRouter(500);
  const pulseUrl = options?.pulseUrl || PULSE_URL;
  const relayUrl = options?.relayUrl !== undefined ? options.relayUrl : BUZZ_RELAY_URL;
  const rolesUrl = options?.rolesUrl || ROLES_URL;
  const boardUrl = options?.boardUrl || BOARD_URL;
  const cardsUrl = options?.cardsUrl || CARDS_URL;
  const alertsUrl = options?.alertsUrl || ALERTS_URL;
  const sessionsUrl = options?.sessionsUrl || SESSIONS_URL;
  const lokiUrl = options?.lokiUrl || LOKI_URL;
  const tileService = new TileService(pulseUrl);
  const keys = options?.knownKeys || KNOWN_KEYS;
  const relayConfigured = options?.relayConfigured !== undefined
    ? options.relayConfigured
    : Boolean(relayUrl);

  // Track SSE clients
  const sseClients: Response[] = [];

  function broadcastSSE(type: string, data: unknown) {
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
  }

  // Handle incoming verified event from Buzz relay
  function onInboundEvent(event: NostrEvent) {
    if (!verifyNostrEvent(event)) {
      return;
    }

    const role = resolveRoleByPubkey(event.pubkey, keys) || 'unattributed';
    const roomTag = event.tags?.find((t) => t[0] === 'room' || (t[0] === 't' && t[1] !== 'reply'));
    const room = roomTag ? roomTag[1] : 'team';
    const msg: ClearingMessage = {
      id: event.id,
      from: role,
      text: event.content,
      ts: new Date(event.created_at * 1000).toISOString(),
      type: role === 'jeff' ? 'jeff-input' : 'role-response',
      visible: true,
      pubkey: event.pubkey,
      room,
      tick: true,
    };

    if (router.ingest(msg)) {
      broadcastSSE('message', msg);

      const nudges = router.extractNudges(msg);
      for (const nudge of nudges) {
        fetch(`${pulseUrl}/api/nudge`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(nudge),
        }).catch(() => {});
      }
    }
  }

  const relay = new BuzzRelayClient({
    relayUrl,
    topic: 'team',
    onEvent: onInboundEvent,
  });

  const dataService = new DataService({
    rolesUrl,
    boardUrl,
    cardsUrl,
    alertsUrl,
    sessionsUrl,
    lokiUrl,
    pulseUrl,
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
    relayUrl,
    pulseUrl,
    broadcastSSE,
    onInboundEvent,
    keys,
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
      console.log(`Buzz relay not configured (trust strip shows rooms failed: relay not configured)`);
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
