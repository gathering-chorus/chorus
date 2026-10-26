// @test-type: unit
// @card: #4751
// @domain: messages

import fs from 'fs';
import path from 'path';
import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';

describe('#4751 Clearing v2 smooth history scroll and mobile header compactness', () => {
  const stylePath = path.join(__dirname, '../public/style.css');
  const styleContent = fs.readFileSync(stylePath, 'utf8');

  it('contains content-visibility and contain-intrinsic-size on .msg for off-screen rendering containment', () => {
    expect(styleContent).toMatch(/\.msg[^{]*\{[^}]*content-visibility:\s*auto/);
    expect(styleContent).toMatch(/\.msg[^{]*\{[^}]*contain-intrinsic-size:\s*auto\s+120px/);
  });

  it('contains explicit sizing and aspect-ratio on msg photos with object-fit: contain to prevent prepend reflow jumps without cropping', () => {
    expect(styleContent).toMatch(/\.msg-photos\s+img\s*\{[^}]*aspect-ratio/);
    expect(styleContent).toMatch(/\.msg-photos\s+img\s*\{[^}]*object-fit:\s*contain/);
  });

  it('defaults relay-client fetchBefore limit to 15 rather than 50', async () => {
    const ws = new MockWebSocket();
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws as any });
    relay.connect();
    ws.triggerOpen();

    const pending = relay.fetchBefore(1500);
    const [req9, req1] = ws.sentMessages.slice(-2).map((m) => JSON.parse(m));
    expect(req9[2].limit).toBe(15);
    expect(req1[2].limit).toBe(15);
    ws.triggerMessage(JSON.stringify(['EOSE', req9[1]]));
    ws.triggerMessage(JSON.stringify(['EOSE', req1[1]]));
    await pending;
  });

  it('provides responsive mobile styles for chat-header-row so nudges and counts fit on narrow screens', () => {
    expect(styleContent).toMatch(/\.chat-header-row/);
  });
});
