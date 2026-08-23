// @test-type: unit — pure env resolution; no port opened, no io.
/**
 * #3967 — pulse was network-exposed by omission (`app.listen(PORT)` binds
 * 0.0.0.0), so its anonymous write routes were reachable from the whole LAN.
 * The fix binds loopback by default. resolveBindHost must DEFAULT to 127.0.0.1
 * and open the network ONLY on an explicit CHORUS_BIND override.
 *
 * NEGATIVE PROOF (#3734): if the default were 0.0.0.0 (or empty → express's
 * all-interfaces default), the gap would silently return. Asserting the
 * loopback default makes that regression fail here rather than in a coverage
 * scan weeks later.
 */
import { resolveBindHost } from './service';

describe('#3967 pulse bind host', () => {
  it('defaults to loopback — not the network', () => {
    expect(resolveBindHost({} as NodeJS.ProcessEnv)).toBe('127.0.0.1');
  });

  it('NEGATIVE PROOF: the default is never 0.0.0.0 / all-interfaces', () => {
    const host = resolveBindHost({} as NodeJS.ProcessEnv);
    expect(host).not.toBe('0.0.0.0');
    expect(host).not.toBe('');
    expect(host).not.toBe('::');
  });

  it('opens the network only on an explicit CHORUS_BIND override', () => {
    expect(resolveBindHost({ CHORUS_BIND: '0.0.0.0' } as NodeJS.ProcessEnv)).toBe('0.0.0.0');
    // #3991: fixture IP kept OFF the home-LAN subnet — the #3370 ratchet
    // rightly flags any new literal on that range; pass-through needs any IP.
    expect(resolveBindHost({ CHORUS_BIND: '10.20.30.40' } as NodeJS.ProcessEnv)).toBe('10.20.30.40');
  });
});
