// @test-type: unit
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
// @card: #4412
// @owner: wren
/**
 * #4412 — Jeff, 2026-10-11: Account showed a sign-in button that 404'd on the
 * apex. (That sign-in is gone: one sign-in for Chorus is the door's.) The account and refusal
 * pages carried root-absolute links, so under the /clearing mount the button
 * went to the apex and 404'd. Every page the server writes now gets the prefix.
 */
import { withBase, withBaseLocation, safeReturnPath } from '../src/server';

const SIGN_IN = '<html><body><a href="/account?from=%2Froom"><button>Account</button></a>'
  + '<form action="/api/account/password"></form>'
  + "<script>fetch('/api/account/password',{method:'POST'})</script></body></html>";

describe('#4412 auth pages under the /clearing mount', () => {
  it('points the account link at the mount', () => {
    expect(withBase(SIGN_IN, '/clearing')).toContain('href="/clearing/account?from=%2Froom"');
  });

  it('prefixes form posts and fetches the page makes', () => {
    const out = withBase(SIGN_IN, '/clearing');
    expect(out).toContain('action="/clearing/api/account/password"');
    expect(out).toContain("fetch('/clearing/api/account/password'");
  });

  // NEGATIVE PROOF: without the prefix the button is the one Jeff hit
  it('NEGATIVE PROOF — on the clearing host and localhost nothing changes', () => {
    expect(withBase(SIGN_IN, '')).toBe(SIGN_IN);
    expect(withBase(SIGN_IN, '')).toContain('href="/account?from');
  });

  it('leaves protocol-relative and external URLs alone', () => {
    const page = '<script src="//cdn.example/x.js"></script><a href="https://id.example/">id</a>';
    expect(withBase(page, '/clearing')).toBe(page);
  });
});

describe('#4412 redirects and styles under the /clearing mount (the whole class)', () => {
  // Jeff 09:44: signed in, landed on "Cannot GET /account" — the callback's
  // return path was the bare /account.
  it('a bare return path lands inside the mount', () => {
    expect(withBaseLocation('/account', '/clearing')).toBe('/clearing/account');
    expect(withBaseLocation('/', '/clearing')).toBe('/clearing/');
  });

  it('a path already under the mount is not prefixed twice', () => {
    expect(withBaseLocation('/clearing/account', '/clearing')).toBe('/clearing/account');
    expect(withBaseLocation('/clearing', '/clearing')).toBe('/clearing');
    expect(withBaseLocation('/clearing?x=1', '/clearing')).toBe('/clearing?x=1');
  });

  it('full URLs and protocol-relative targets are left alone', () => {
    expect(withBaseLocation('https://id.example/.oidc/auth?x=1', '/clearing')).toBe('https://id.example/.oidc/auth?x=1');
    expect(withBaseLocation('//evil.example/x', '/clearing')).toBe('//evil.example/x');
  });

  it('NEGATIVE PROOF — on the clearing host and localhost redirects are unchanged', () => {
    expect(withBaseLocation('/account', '')).toBe('/account');
  });

  it('a CSS background url gets the mount (clearing-bg.jpg 404d on the apex)', () => {
    const css = "background-image: linear-gradient(#000,#000), url('/clearing-bg.jpg');";
    expect(withBase(css.replace(/^/, '<style>') + '</style>', '/clearing')).toContain("url('/clearing/clearing-bg.jpg')");
    expect(withBase('<style>a{background:url(/x.png)}</style>', '/clearing')).toContain('url(/clearing/x.png)');
  });
});

// Moved from solid-oidc.test.ts (#3669) when the Clearing's own OIDC module was deleted.
describe('#3669 safeReturnPath — open-redirect guard', () => {
  test('a same-origin path survives', () => {
    expect(safeReturnPath('/api/chat')).toBe('/api/chat');
    expect(safeReturnPath('/')).toBe('/');
  });
  test('NEGATIVE PROOF: anything that could leave the origin becomes /', () => {
    expect(safeReturnPath('https://evil.com')).toBe('/');
    expect(safeReturnPath('//evil.com')).toBe('/');
    expect(safeReturnPath('/\\evil.com')).toBe('/');
    expect(safeReturnPath('javascript:alert(1)')).toBe('/');
    expect(safeReturnPath('')).toBe('/');
    expect(safeReturnPath(undefined)).toBe('/');
  });
});
