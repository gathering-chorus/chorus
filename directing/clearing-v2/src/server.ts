// @domain: messages

import express, { Request, Response } from 'express';
import path from 'path';
import { MessageRouter } from './router';
import { TileService } from './tile-service';
import { BuzzRelayClient } from './relay-client';
import { verifyNostrEvent, resolveRoleByPubkey } from './identity';
import { ClearingMessage, NostrEvent } from './types';

const PORT = parseInt(process.env.PORT || '3472', 10);
const HOST = process.env.HOST || '127.0.0.1';
const BUZZ_RELAY_URL = process.env.BUZZ_RELAY_URL || 'ws://192.168.86.242:3000';
const PULSE_URL = process.env.PULSE_URL || 'http://127.0.0.1:3475';

// Known pubkeys map for role attribution
const KNOWN_KEYS = new Map<string, string>([
  ['b9d4bfd85efd8aeeacaef985d4e12325cb1d7d527755be875a3813836e06d9b7', 'abby-normal'],
]);

export function createApp(options?: {
  relayUrl?: string;
  pulseUrl?: string;
  knownKeys?: Map<string, string>;
}) {
  const app = express();
  app.use(express.json());
  app.use(express.static(path.join(__dirname, '../public')));

  const router = new MessageRouter(500);
  const tileService = new TileService(options?.pulseUrl || PULSE_URL);
  const keys = options?.knownKeys || KNOWN_KEYS;

  // Track SSE clients
  const sseClients: Response[] = [];

  function broadcastSSE(type: string, data: any) {
    const payload = `event: ${type}\ndata: ${JSON.stringify(data)}\n\n`;
    for (let i = sseClients.length - 1; i >= 0; i--) {
      try {
        sseClients[i].write(payload);
      } catch {
        sseClients.splice(i, 1);
      }
    }
  }

  // Handle incoming verified event from Buzz relay
  function onInboundEvent(event: NostrEvent) {
    if (!verifyNostrEvent(event)) {
      return; // Drop invalid cryptographic signatures
    }

    const role = resolveRoleByPubkey(event.pubkey, keys) || 'unattributed';
    const msg: ClearingMessage = {
      id: event.id,
      from: role,
      text: event.content,
      ts: new Date(event.created_at * 1000).toISOString(),
      type: role === 'jeff' ? 'jeff-input' : 'role-response',
      visible: true,
      pubkey: event.pubkey,
    };

    if (router.ingest(msg)) {
      broadcastSSE('message', msg);

      // Check for mentions and forward nudge to Pulse
      const nudges = router.extractNudges(msg);
      for (const nudge of nudges) {
        fetch(`${options?.pulseUrl || PULSE_URL}/api/nudge`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(nudge),
        }).catch(() => {});
      }
    }
  }

  const relay = new BuzzRelayClient({
    relayUrl: options?.relayUrl || BUZZ_RELAY_URL,
    topic: 'team',
    onEvent: onInboundEvent,
  });

  // Endpoints
  app.get('/health', (_req: Request, res: Response) => {
    res.json({
      status: 'ok',
      ok: true,
      service: 'clearing-v2',
      version: '2.0.0',
      port: PORT,
      relayConnected: relay.isConnected(),
      relayUrl: options?.relayUrl || BUZZ_RELAY_URL,
    });
  });

  app.get('/api/messages', (_req: Request, res: Response) => {
    res.json(router.getRecent(100));
  });

  app.get('/api/tiles', async (_req: Request, res: Response) => {
    const tiles = await tileService.refresh();
    res.json(tiles);
  });

  app.post('/api/send', (req: Request, res: Response) => {
    const { from, text, event } = req.body;

    if (event && verifyNostrEvent(event)) {
      // Direct signed Nostr event from client
      relay.publish(event);
      onInboundEvent(event);
      res.json({ ok: true, id: event.id });
      return;
    }

    if (!text || typeof text !== 'string') {
      res.status(400).json({ ok: false, error: 'text is required' });
      return;
    }

    // Local unsigned message from browser UI (Jeff)
    const msg: ClearingMessage = {
      id: `local-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      from: from || 'jeff',
      text: text.trim(),
      ts: new Date().toISOString(),
      type: 'jeff-input',
      visible: true,
    };

    router.ingest(msg);
    broadcastSSE('message', msg);

    // Forward mentions to Pulse
    const nudges = router.extractNudges(msg);
    for (const nudge of nudges) {
      fetch(`${options?.pulseUrl || PULSE_URL}/api/nudge`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(nudge),
      }).catch(() => {});
    }

    res.json({ ok: true, id: msg.id });
  });

  app.get('/api/events', (req: Request, res: Response) => {
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');
    res.flushHeaders();

    sseClients.push(res);

    // Send initial snapshot
    res.write(`event: init\ndata: ${JSON.stringify({ messages: router.getRecent(50), tiles: tileService.getTiles() })}\n\n`);

    req.on('close', () => {
      const idx = sseClients.indexOf(res);
      if (idx !== -1) sseClients.splice(idx, 1);
    });
  });

  return { app, router, tileService, relay };
}

// Start standalone if executed directly
if (require.main === module) {
  const { app, relay } = createApp();
  relay.connect();

  app.listen(PORT, HOST, () => {
    console.log(`Clearing v2 running at http://${HOST}:${PORT}`);
    console.log(`Connected to Buzz relay at ${BUZZ_RELAY_URL}`);

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
