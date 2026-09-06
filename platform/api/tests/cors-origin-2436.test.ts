// @test-type: unit:security — pure function over Origin strings; no server, no HTTP. The api/URL words are fixture-data (the origins under test), not an integration surface.
/**
 * #2436 — the CORS allow-list must separate three states, not two.
 *
 * The live defect: `/api/athena/*` answered `Allow-Origin: *` with
 * GET/POST/PUT/DELETE, and `/api/chorus/open` answered `*` with POST. Since
 * chorus-api binds 0.0.0.0:3340, any page in any browser could write.
 *
 * The states this has to keep apart:
 *   this machine (any port)   -> echo the origin
 *   anywhere else             -> NO header at all
 *   no Origin (server call)   -> NO header, and unaffected either way
 *
 * The middle case is the one a wildcard could never express, and the one every
 * probe below exists to prove is reachable.
 */
import { corsAllowOrigin } from '../src/cors-origin';

describe('#2436 corsAllowOrigin — who may talk to chorus-api from a browser', () => {
  describe('granted: this machine, on whatever port it happens to serve', () => {
    it.each([
      'http://localhost:3000',
      'http://localhost:3343',
      'http://127.0.0.1:3340',
      'http://[::1]:3000',
      'https://localhost:8443',
    ])('echoes %s', (origin) => {
      expect(corsAllowOrigin(origin)).toBe(origin);
    });

    it('echoes the LAN address Jeff reads demos from, without pinning it', () => {
      // ADR-012: the Library's address is DHCP-volatile, so the rule is the
      // range, not the host. Both machines must pass on any port.
      expect(corsAllowOrigin('http://192.168.86.36:3343')).toBe('http://192.168.86.36:3343');
      expect(corsAllowOrigin('http://192.168.86.242:3000')).toBe('http://192.168.86.242:3000');
    });

    it.each(['http://10.0.0.5:3000', 'http://172.16.4.4:3000', 'http://169.254.1.1:3000'])(
      'echoes other private ranges: %s',
      (origin) => {
        expect(corsAllowOrigin(origin)).toBe(origin);
      },
    );
  });

  describe('NEGATIVE PROOF: refused — the state a wildcard could not express', () => {
    it.each([
      'https://evil.example',
      'http://evil.example:3000',
      'http://8.8.8.8:3000',
      'http://172.32.0.1:3000', // just outside 172.16–31
      'http://192.169.0.1:3000', // just outside 192.168
    ])('sends no header for %s', (origin) => {
      expect(corsAllowOrigin(origin)).toBeNull();
    });

    it('is not fooled by a public host that merely contains a private address', () => {
      expect(corsAllowOrigin('http://192.168.86.36.evil.example')).toBeNull();
      expect(corsAllowOrigin('http://localhost.evil.example')).toBeNull();
    });

    it('refuses a non-http scheme', () => {
      expect(corsAllowOrigin('file://localhost')).toBeNull();
      expect(corsAllowOrigin('javascript:alert(1)')).toBeNull();
    });

    it('refuses an unparseable origin rather than echoing it back', () => {
      expect(corsAllowOrigin('not a url')).toBeNull();
      expect(corsAllowOrigin('*')).toBeNull();
    });

    it('refuses an out-of-range octet that still looks numeric', () => {
      expect(corsAllowOrigin('http://192.168.999.1:3000')).toBeNull();
    });
  });

  describe('server-to-server callers are untouched', () => {
    it('grants nothing when there is no Origin — nothing to grant', () => {
      expect(corsAllowOrigin(undefined)).toBeNull();
      expect(corsAllowOrigin(null)).toBeNull();
      expect(corsAllowOrigin('')).toBeNull();
    });
  });
});
