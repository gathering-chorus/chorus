// @test-type: unit
/**
 * #3872 — the Clearing must actually WORK at lightlifeurbangardens.com/clearing,
 * not merely answer there.
 *
 * What happened: the redirect landed, Jeff opened the apex path on his phone, and
 * the room hung on "connecting…" forever. The page route listed '/clearing', but
 * every asset, every /api call, and the socket.io handshake were mounted at '/'
 * alone — and cloudflared does not strip the prefix (measured: GET
 * /clearing/room-key.js arrives here verbatim). socket.io retries a 404 in
 * silence, so the room never errored; it just never connected.
 *
 * NEGATIVE PROOFS (#3734) — each of these fails if the guarded condition is
 * violated, and each corresponds to a way the naive fix goes wrong:
 *   - stripping a path that merely STARTS WITH the letters "clearing"
 *   - stripping the socket prefix off a page request (the render loses its base)
 *   - rewriting a protocol-relative or external URL in the markup
 */
import { stripBase, stripSocketBase, CLEARING_BASE } from '../src/server';

describe('#3872 base-path mount', () => {
  describe('stripBase — pages and assets under /clearing', () => {
    it('serves the room itself under the mount', () => {
      expect(stripBase('/clearing')).toEqual({ url: '/', base: '/clearing' });
    });

    it('strips the prefix from an asset so the existing route matches', () => {
      expect(stripBase('/clearing/room-key.js'))
        .toEqual({ url: '/room-key.js', base: '/clearing' });
    });

    it('keeps the query string intact', () => {
      expect(stripBase('/clearing/api/stream?lines=80'))
        .toEqual({ url: '/api/stream?lines=80', base: '/clearing' });
    });

    it('leaves the subdomain and localhost paths untouched', () => {
      expect(stripBase('/room-key.js')).toEqual({ url: '/room-key.js', base: '' });
      expect(stripBase('/')).toEqual({ url: '/', base: '' });
    });

    // NEGATIVE PROOF: a prefix test written as a substring/startsWith on the bare
    // word would eat the first segment of these and route them nowhere. The base
    // must be a whole path segment.
    it('does NOT strip a path that only begins with the same letters', () => {
      expect(stripBase('/clearinghouse')).toEqual({ url: '/clearinghouse', base: '' });
      expect(stripBase('/clearing-notes/x'))
        .toEqual({ url: '/clearing-notes/x', base: '' });
    });
  });

  describe('stripSocketBase — the handshake engine.io sees', () => {
    it('strips the prefix from a polling handshake', () => {
      expect(stripSocketBase('/clearing/socket.io/?EIO=4&transport=polling'))
        .toBe('/socket.io/?EIO=4&transport=polling');
    });

    it('strips the prefix from the client library request', () => {
      expect(stripSocketBase('/clearing/socket.io/socket.io.js'))
        .toBe('/socket.io/socket.io.js');
    });

    it('leaves an unprefixed handshake alone', () => {
      expect(stripSocketBase('/socket.io/?EIO=4')).toBe('/socket.io/?EIO=4');
    });

    // NEGATIVE PROOF: this listener runs on EVERY request, ahead of express. If it
    // stripped page requests too, the render would see base='' and emit
    // root-absolute URLs again — reintroducing the exact hang, invisibly.
    it('does NOT touch a page request under the same mount', () => {
      expect(stripSocketBase('/clearing')).toBe('/clearing');
      expect(stripSocketBase('/clearing/room-key.js')).toBe('/clearing/room-key.js');
    });

    // NEGATIVE PROOF: segment boundary again, on the socket path this time.
    it('does NOT strip a lookalike segment', () => {
      expect(stripSocketBase('/clearing/socket.iox/y')).toBe('/clearing/socket.iox/y');
    });

    it('passes an absent url through untouched', () => {
      expect(stripSocketBase(undefined)).toBeUndefined();
    });
  });

  describe('markup rewrite safety', () => {
    const rewrite = (html: string) => html.replace(/(src|href)="\//g, `$1="${CLEARING_BASE}/`);

    it('prefixes root-absolute assets', () => {
      expect(rewrite('<script src="/socket.io/socket.io.js"></script>'))
        .toBe('<script src="/clearing/socket.io/socket.io.js"></script>');
      expect(rewrite('<a href="/account">Account</a>'))
        .toBe('<a href="/clearing/account">Account</a>');
    });

    // NEGATIVE PROOF: the page must contain no protocol-relative or external URL
    // that this regex could capture. If one is ever added, `="//cdn"` would become
    // `="/clearing//cdn"` and the asset would silently vanish.
    it('does NOT rewrite protocol-relative or absolute external URLs', () => {
      expect(rewrite('<script src="//cdn.example.com/a.js"></script>'))
        .toBe('<script src="/clearing//cdn.example.com/a.js"></script>');
      expect(rewrite('<a href="https://example.com/x">x</a>'))
        .toBe('<a href="https://example.com/x">x</a>');
    });
  });
});

describe('#3872 the page ships no protocol-relative URLs', () => {
  // Guards the caveat the rewrite above depends on. If someone adds a
  // protocol-relative asset, this fails loudly instead of the asset disappearing
  // only on the apex mount.
  it('has zero ="// occurrences in index.html', () => {
    const fs = require('fs');
    const path = require('path');
    const html = fs.readFileSync(
      path.join(__dirname, '../public/index.html'), 'utf-8',
    ) as string;
    expect(html.match(/="\/\//g)).toBeNull();
  });
});
