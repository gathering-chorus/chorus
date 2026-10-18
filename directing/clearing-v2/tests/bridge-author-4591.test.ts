// @test-type: unit — verifies bridge author unwrap and dedupe retention
// @card: #4591
// @owner: abby-normal
// @domain: messages

import { makeInboundHandler } from '../src/server';
import { MessageRouter } from '../src/router';
import { NostrEvent } from '../src/types';
import { schnorr } from '@noble/curves/secp256k1';
import { sha256 } from '@noble/hashes/sha256';
import { bytesToHex, hexToBytes, utf8ToBytes } from '@noble/hashes/utils';
const { appendMessage } = require('../public/app.js');

function makeSignedEvent(skHex: string, content: string, tags: string[][] = []): NostrEvent {
  const sk = hexToBytes(skHex);
  const pubkey = bytesToHex(schnorr.getPublicKey(sk));
  const created_at = 1786464000;
  const kind = 1;
  const serialized = JSON.stringify([0, pubkey.toLowerCase(), created_at, kind, tags, content]);
  const id = bytesToHex(sha256(utf8ToBytes(serialized)));
  const sig = bytesToHex(schnorr.sign(hexToBytes(id), sk));
  return { id, pubkey, created_at, kind, tags, content, sig };
}

describe('#4591: Bridge author unwrapping and message deduplication', () => {
  const bridgeSk = '11'.repeat(32);
  const bridgePk = bytesToHex(schnorr.getPublicKey(hexToBytes(bridgeSk)));
  const strangerSk = '22'.repeat(32);
  const strangerPk = bytesToHex(schnorr.getPublicKey(hexToBytes(strangerSk)));

  const keys = new Map<string, string>([
    [bridgePk.toLowerCase(), 'bridge'],
    [strangerPk.toLowerCase(), 'stranger'],
  ]);

  test('a bridge-signed event with an author tag is attributed to that author', () => {
    const router = new MessageRouter();
    const handler = makeInboundHandler({
      router,
      keys,
      pulseUrl: 'http://127.0.0.1:9999',
      broadcastSSE: jest.fn(),
      isRelayConnected: () => true,
    });

    const event = makeSignedEvent(bridgeSk, 'Plan ready: link', [['t', 'team'], ['author', 'wren']]);
    handler(event);

    const recent = router.getRecent(10);
    expect(recent.length).toBe(1);
    expect(recent[0].from).toBe('wren');
    expect(recent[0].text).toBe('Plan ready: link');
  });

  test('NEGATIVE PROOF: an author tag on an event signed by a non-bridge key is ignored', () => {
    const router = new MessageRouter();
    const handler = makeInboundHandler({
      router,
      keys,
      pulseUrl: 'http://127.0.0.1:9999',
      broadcastSSE: jest.fn(),
      isRelayConnected: () => true,
    });

    // Stranger signs an event claiming to be 'wren'
    const event = makeSignedEvent(strangerSk, 'Spoofed message', [['t', 'team'], ['author', 'wren']]);
    handler(event);

    const recent = router.getRecent(10);
    expect(recent.length).toBe(1);
    // Must remain 'stranger', NOT 'wren'
    expect(recent[0].from).toBe('stranger');
    expect(recent[0].from).not.toBe('wren');
  });

  test('frontend dedupe does not drop distinct messages sharing identical text', () => {
    const router = new MessageRouter();
    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    (global as any).document = {
      getElementById: (id: string) => (id === 'messages-stream' ? container : null),
      createElement: (tag: string) => {
        const el: any = {
          tag,
          className: '',
          id: '',
          textContent: '',
          children: [],
          dataset: {},
          appendChild: (c: any) => el.children.push(c),
          querySelector: (sel: string) => el.children.find((c: any) => sel.includes(c.className)),
        };
        return el;
      },
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };

    // First message: "hello"
    appendMessage({
      id: 'msg-1',
      from: 'jeff',
      text: 'hello',
      ts: '2026-10-17T18:00:00.000Z',
      visible: true,
    });

    // Second message: distinct id 'msg-2', identical text "hello"
    appendMessage({
      id: 'msg-2',
      from: 'jeff',
      text: 'hello',
      ts: '2026-10-17T18:01:00.000Z',
      visible: true,
    });

    expect(container.children.length).toBe(2);
    expect(container.children[0].id).toBe('msg-msg-1');
    expect(container.children[1].id).toBe('msg-msg-2');
    expect(router.getRecent(1)).toEqual([]);
  });

  test('NEGATIVE PROOF: identical text replaces bridge placeholder when real copy arrives', () => {
    const router = new MessageRouter();
    const container: any = { children: [], appendChild: (c: any) => container.children.push(c) };
    (global as any).document = {
      getElementById: (id: string) => (id === 'messages-stream' ? container : null),
      createElement: (tag: string) => {
        const el: any = {
          tag,
          className: '',
          id: '',
          textContent: '',
          children: [],
          dataset: {},
          appendChild: (c: any) => el.children.push(c),
          querySelector: (sel: string) => el.children.find((c: any) => sel.includes(c.className)),
        };
        return el;
      },
      createTextNode: (t: string) => ({ tag: '#text', textContent: t }),
    };

    // 1. Initial message rendered as bridge
    appendMessage({
      id: 'bridge-1',
      from: 'bridge',
      text: 'syncing up',
      ts: '2026-10-17T18:00:00.000Z',
      visible: true,
    });
    expect(container.children.length).toBe(1);
    expect(container.children[0].dataset.from).toBe('bridge');

    // 2. Real copy arrives from 'jeff' with same text
    appendMessage({
      id: 'real-1',
      from: 'jeff',
      text: 'syncing up',
      ts: '2026-10-17T18:00:01.000Z',
      visible: true,
    });

    // Count stays 1, placeholder is upgraded
    expect(container.children.length).toBe(1);
    expect(container.children[0].dataset.from).toBe('jeff');
    expect(container.children[0].id).toBe('msg-real-1');
    expect(router.getRecent(1)).toEqual([]);
  });

  test('bridge-signed note with author jeff does NOT trigger deliverJeffInput to pulse', () => {
    const router = new MessageRouter();
    const pulseCalls: string[] = [];
    // Mock global fetch to observe pulse calls
    (global as any).fetch = jest.fn().mockImplementation((url: string) => {
      pulseCalls.push(url);
      return Promise.resolve({ ok: true });
    });

    const handler = makeInboundHandler({
      router,
      keys,
      pulseUrl: 'http://127.0.0.1:9999',
      broadcastSSE: jest.fn(),
      isRelayConnected: () => true,
    });

    const event = makeSignedEvent(bridgeSk, '@wren test input', [['t', 'team'], ['author', 'jeff']]);
    handler(event);

    expect(pulseCalls.length).toBe(0);
  });

  test('router drops bridge-signed echo when same author and body already exist in room', () => {
    const router = new MessageRouter();
    // 1. Direct send from Jeff
    const directIngested = router.ingest({
      id: 'jeff-direct-1',
      from: 'jeff',
      text: 'hello team',
      ts: '2026-10-17T18:00:00.000Z',
      type: 'jeff-input',
      visible: true,
      signer: 'jeff',
    });
    expect(directIngested).toBe(true);

    // 2. Bridge echo arrives with new id but same body and author
    const echoIngested = router.ingest({
      id: 'bridge-echo-1',
      from: 'jeff',
      text: 'hello team',
      ts: '2026-10-17T18:00:01.000Z',
      type: 'jeff-input',
      visible: true,
      signer: 'bridge',
    });
    expect(echoIngested).toBe(false);

    expect(router.getRecent(10).length).toBe(1);
    expect(router.getRecent(10)[0].id).toBe('jeff-direct-1');
  });
});

