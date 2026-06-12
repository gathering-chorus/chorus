/**
 * #3366 — LAN IP drift broke wifi access to the Clearing.
 *
 * Jeff's experience under test: the startup banner gives him a LAN URL that
 * keeps working after DHCP hands the Mac a new address — and when the address
 * does move, the drift is visible (logged + spine breadcrumb), never silent.
 */
import * as os from 'os';
import { lanAddress, bonjourHost, startupLanLines, detectIpDrift } from '../src/lan-url';

function iface(address: string, family: 'IPv4' | 'IPv6' = 'IPv4', internal = false): os.NetworkInterfaceInfo {
  return { address, family, internal, netmask: '', mac: '', cidr: null } as os.NetworkInterfaceInfo;
}

describe('lanAddress', () => {
  it('returns the 192.168.86.x IPv4 address', () => {
    const ifaces = {
      lo0: [iface('127.0.0.1', 'IPv4', true)],
      en0: [iface('fe80::1', 'IPv6'), iface('192.168.86.23')],
    };
    expect(lanAddress(ifaces)).toBe('192.168.86.23');
  });

  it('ignores internal, IPv6, and non-LAN addresses', () => {
    const ifaces = {
      lo0: [iface('127.0.0.1', 'IPv4', true)],
      en0: [iface('10.0.0.5'), iface('fe80::2', 'IPv6')],
    };
    expect(lanAddress(ifaces)).toBeNull();
  });

  it('returns null when no interfaces exist', () => {
    expect(lanAddress({})).toBeNull();
  });
});

describe('bonjourHost', () => {
  it('lowercases the LocalHostName and appends .local', () => {
    expect(bonjourHost('Jeffs-Mac-Mini-M1-3')).toBe('jeffs-mac-mini-m1-3.local');
  });

  it('strips a domain suffix from a fallback hostname', () => {
    expect(bonjourHost('macmini.lan')).toBe('macmini.local');
  });

  it('returns null for empty input', () => {
    expect(bonjourHost('')).toBeNull();
    expect(bonjourHost(null)).toBeNull();
  });
});

describe('startupLanLines — what Jeff reads in the log', () => {
  const ifaces = { en0: [iface('192.168.86.23')] };

  it('gives an IP-proof .local URL as the canonical LAN address', () => {
    const lines = startupLanLines(3470, ifaces, 'Jeffs-Mac-Mini-M1-3');
    expect(lines.join('\n')).toContain('http://jeffs-mac-mini-m1-3.local:3470');
  });

  it('also prints the current numeric IP so a non-mDNS client still has a path', () => {
    const lines = startupLanLines(3470, ifaces, 'Jeffs-Mac-Mini-M1-3');
    expect(lines.join('\n')).toContain('http://192.168.86.23:3470');
  });

  it('never hardcodes a stale address — the dead .36 cannot reappear', () => {
    const lines = startupLanLines(3470, ifaces, 'Jeffs-Mac-Mini-M1-3');
    expect(lines.join('\n')).not.toContain('192.168.86.36');
  });

  it('says so plainly when no LAN address is up instead of printing a wrong URL', () => {
    const lines = startupLanLines(3470, {}, 'Jeffs-Mac-Mini-M1-3');
    const joined = lines.join('\n');
    expect(joined).toContain('no LAN address');
    expect(joined).not.toMatch(/http:\/\/192\.168/);
  });
});

describe('detectIpDrift — the breadcrumb when DHCP moves the machine', () => {
  it('flags a change from one LAN address to another', () => {
    expect(detectIpDrift('192.168.86.36', '192.168.86.23')).toEqual({
      drifted: true,
      from: '192.168.86.36',
      to: '192.168.86.23',
    });
  });

  it('is quiet on first boot (no previous record)', () => {
    expect(detectIpDrift(null, '192.168.86.23').drifted).toBe(false);
  });

  it('is quiet when the address is unchanged', () => {
    expect(detectIpDrift('192.168.86.23', '192.168.86.23').drifted).toBe(false);
  });

  it('is quiet when the machine currently has no LAN address (offline is not drift)', () => {
    expect(detectIpDrift('192.168.86.23', null).drifted).toBe(false);
  });
});
