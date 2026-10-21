// @domain: messages
// #4652 reopen 2 — store a post Jeff typed in Clearing in Buzz as Jeff (A2, 10-20 11:53).
// The relay stores an event only when its pubkey is the connection's AUTH identity
// (#4608, measured 10-18), so each post goes over its own connection: the signing
// service signs the post and the AUTH answer as Jeff, then the post is sent and the
// relay's OK read. Any failure means "not sent"; nothing goes out under the room's key.
import { SignResult, UnsignedEvent } from './remote-signer';
import { authRelayFor, WebSocketLike } from './relay-client';
import { NostrEvent } from './types';

export interface EventSigner { signEvent(unsigned: UnsignedEvent): Promise<SignResult> }

export interface JeffPublisherOptions {
  relayUrl: string;
  signer: EventSigner;
  /** The relay is host-scoped (v1: "connect via the LAN IP, never localhost"). */
  hostHeader?: string;
  wsFactory?: () => WebSocketLike;
  timeoutMs?: number;
}

export type PublishResult = { ok: true; event: NostrEvent } | { ok: false; reason: string };

export class JeffPublisher {
  constructor(private readonly o: JeffPublisherOptions) {}

  async publish(unsigned: UnsignedEvent): Promise<PublishResult> {
    const signed = await this.o.signer.signEvent(unsigned);
    if (!signed.ok) return { ok: false, reason: signed.reason };
    return this.send(signed.event);
  }

  /** The relay's own address for the AUTH tag; through the tunnel that is the Host it serves (as the room's AUTH does). */
  private authRelayUrl(): string {
    return authRelayFor(this.o.relayUrl, this.o.hostHeader);
  }

  private open(): WebSocketLike {
    if (this.o.wsFactory) return this.o.wsFactory();
    const WebSocket = require('ws');
    return this.o.hostHeader ? new WebSocket(this.o.relayUrl, { headers: { Host: this.o.hostHeader } }) : new WebSocket(this.o.relayUrl);
  }

  /** One connection: answer the relay's challenge as Jeff, send the post, read its OK. */
  private send(event: NostrEvent): Promise<PublishResult> {
    return new Promise((resolve) => {
      const ws = this.open();
      let authId = '';
      let done = false;
      const finish = (r: PublishResult) => { if (done) return; done = true; clearTimeout(timer); try { ws.close(); } catch { /* closing */ } resolve(r); };
      const timer = setTimeout(() => finish({ ok: false, reason: `relay did not confirm within ${this.o.timeoutMs ?? 10000} ms` }), this.o.timeoutMs ?? 10000);
      ws.on('error', () => finish({ ok: false, reason: 'relay connection failed' }));
      ws.on('message', (raw: unknown) => {
        let m: unknown[];
        try { m = JSON.parse(String(raw)); } catch { return; }
        if (m[0] === 'AUTH' && typeof m[1] === 'string') {
          const challenge = m[1];
          void this.o.signer.signEvent({ kind: 22242, created_at: Math.floor(Date.now() / 1000), tags: [['relay', this.authRelayUrl()], ['challenge', challenge]], content: '' }).then((a) => {
            if (!a.ok) { finish({ ok: false, reason: `relay sign-in not signed: ${a.reason}` }); return; }
            authId = a.event.id;
            ws.send(JSON.stringify(['AUTH', a.event]));
          });
        } else if (m[0] === 'OK' && m[1] === authId) {
          if (m[2] !== true) { finish({ ok: false, reason: `relay refused sign-in: ${typeof m[3] === 'string' ? m[3] : ''}` }); return; }
          ws.send(JSON.stringify(['EVENT', event]));
        } else if (m[0] === 'OK' && m[1] === event.id) {
          finish(m[2] === true ? { ok: true, event } : { ok: false, reason: `relay refused the post: ${typeof m[3] === 'string' ? m[3] : ''}` });
        }
      });
    });
  }
}
