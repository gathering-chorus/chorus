// @test-type: unit — pure functions; no CSS, no network.
// @card: #4412
// @owner: wren
/**
 * #4412 — a LAN browser signs in like anywhere else, and its callback is the
 * address it came in on, never the phone's own localhost.
 */
import { lanOrigins, callbackFor, lanAccess } from '../src/lan-sign-in';

const TUNNEL = 'https://clearing.lightlifeurbangardens.com/auth/callback';
const ORIGINS = lanOrigins('jeffs-mac-mini-m1-3', '192.168.86.23', 3470);

describe('#4412 — the LAN callback follows the Host the browser used', () => {
  it('NEGATIVE PROOF — a dotted hostname never becomes name.lan.local', () => {
    expect(lanOrigins('jeffs-mini-m1-3.lan', null, 3481)).toEqual(['http://jeffs-mini-m1-3.local:3481']);
  });
  it('lists the .local name and the current IP', () => {
    expect(ORIGINS).toEqual(['http://jeffs-mac-mini-m1-3.local:3470', 'http://192.168.86.23:3470']);
  });
  it('a phone on the IP URL comes back to the IP URL', () => {
    expect(callbackFor('192.168.86.23:3470', false, ORIGINS, 3470, TUNNEL)).toBe('http://192.168.86.23:3470/auth/callback');
  });
  it('the .local URL comes back to the .local URL', () => {
    expect(callbackFor('jeffs-mac-mini-m1-3.local:3470', false, ORIGINS, 3470, TUNNEL))
      .toBe('http://jeffs-mac-mini-m1-3.local:3470/auth/callback');
  });
  it('tunneled always uses the tunnel callback', () => {
    expect(callbackFor('192.168.86.23:3470', true, ORIGINS, 3470, TUNNEL)).toBe(TUNNEL);
  });
  it('NEGATIVE PROOF — a Host we do not list never becomes the redirect target', () => {
    expect(callbackFor('evil.example:3470', false, ORIGINS, 3470, TUNNEL)).toBe('http://localhost:3470/auth/callback');
  });
});

describe('#4412 — who must sign in on the LAN', () => {
  const page = { flagOn: true, local: true, ip: '192.168.86.40', method: 'GET', accept: 'text/html,*/*', authed: false };
  it('a phone loading a page with no session signs in', () => {
    expect(lanAccess(page)).toBe('sign-in');
  });
  it('NEGATIVE PROOF — a LAN API read with no session is refused, never left open', () => {
    expect(lanAccess({ ...page, accept: 'application/json' })).toBe('refuse');
  });
  it('NEGATIVE PROOF — a LAN write with no session is refused', () => {
    expect(lanAccess({ ...page, method: 'POST' })).toBe('refuse');
  });
  it('off by default: with the flag off nothing changes', () => {
    expect(lanAccess({ ...page, flagOn: false })).toBe('pass');
    expect(lanAccess({ ...page, flagOn: false, accept: 'application/json' })).toBe('pass');
  });
  it('the box itself (loopback) stays trusted', () => {
    expect(lanAccess({ ...page, ip: '127.0.0.1', accept: 'application/json' })).toBe('pass');
    expect(lanAccess({ ...page, ip: '::ffff:127.0.0.1' })).toBe('pass');
  });
  it('a signed-in visitor passes', () => {
    expect(lanAccess({ ...page, authed: true, accept: 'application/json' })).toBe('pass');
  });
});
