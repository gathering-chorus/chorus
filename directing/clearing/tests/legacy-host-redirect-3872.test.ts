// @test-type: unit — pure URL derivation; no server, no socket.
/**
 * #3872 — the upside-down URL is retired, and old links still work.
 *
 * Jeff, 2026-08-14: "having clearing.lightlifeurbangardens.com is upside down.
 * That's not the normal way you form URLs. You go from general to specific...
 * this is basic human needs, and these are interfaces, and it's like 100%
 * afterthought."
 *
 * #3878 made the apex path work but left the subdomain serving the room, so the
 * shape he objected to was still live and still linkable. This retires it.
 *
 * NEGATIVE PROOF (#3734): `the apex host is left alone` is the state a careless
 * regex produces — one that matches "clearing" anywhere would redirect the apex
 * path to itself, forever. That is a loop the browser shows as a dead page, and
 * it is the failure this test exists to make impossible.
 */
import { legacyRedirectTarget } from '../src/server';

describe('#3872 legacy subdomain retirement', () => {
  it('sends the bare subdomain to the apex path', () => {
    expect(legacyRedirectTarget('clearing.lightlifeurbangardens.com', '/'))
      .toBe('https://lightlifeurbangardens.com/clearing');
  });

  it('keeps a deeper path and its query — an old link still lands where it meant to', () => {
    expect(legacyRedirectTarget('clearing.lightlifeurbangardens.com', '/room?tab=streams'))
      .toBe('https://lightlifeurbangardens.com/clearing/room?tab=streams');
  });

  it('NEGATIVE: the apex host is untouched — no redirect loop', () => {
    expect(legacyRedirectTarget('lightlifeurbangardens.com', '/clearing')).toBeNull();
    expect(legacyRedirectTarget('lightlifeurbangardens.com', '/')).toBeNull();
  });

  it('localhost is untouched — the LAN room keeps working', () => {
    expect(legacyRedirectTarget('localhost:3470', '/')).toBeNull();
    expect(legacyRedirectTarget('127.0.0.1:3470', '/clearing')).toBeNull();
  });

  it('a missing Host header is not a redirect', () => {
    expect(legacyRedirectTarget(undefined, '/')).toBeNull();
  });

  it('matching is case-insensitive — hostnames are', () => {
    expect(legacyRedirectTarget('Clearing.LightLifeUrbanGardens.com', '/'))
      .toBe('https://lightlifeurbangardens.com/clearing');
  });
});
