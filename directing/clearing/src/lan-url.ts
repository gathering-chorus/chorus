/**
 * #3366 — LAN address discovery for the startup banner.
 *
 * The Clearing's remote URL was hardcoded to 192.168.86.36; DHCP moved the
 * machine to .23 and every printed/bookmarked URL died silently. These
 * helpers derive the LAN address at boot and prefer the Bonjour .local
 * hostname, which survives DHCP reassignment entirely.
 */
import * as os from 'os';

export const LAN_PREFIX = '192.168.86.';

export function lanAddress(
  ifaces: NodeJS.Dict<os.NetworkInterfaceInfo[]> = os.networkInterfaces(),
): string | null {
  for (const addrs of Object.values(ifaces)) {
    for (const a of addrs ?? []) {
      if (a.family === 'IPv4' && !a.internal && a.address.startsWith(LAN_PREFIX)) {
        return a.address;
      }
    }
  }
  return null;
}

export function bonjourHost(localHostName: string | null): string | null {
  if (!localHostName) return null;
  const base = localHostName.split('.')[0].toLowerCase();
  return base ? `${base}.local` : null;
}

export function startupLanLines(
  port: number,
  ifaces: NodeJS.Dict<os.NetworkInterfaceInfo[]> = os.networkInterfaces(),
  localHostName: string | null = null,
): string[] {
  const lines: string[] = [];
  const host = bonjourHost(localHostName);
  if (host) {
    lines.push(`[clearing] LAN URL (IP-proof, canonical): http://${host}:${port}`);
  }
  const ip = lanAddress(ifaces);
  if (ip) {
    lines.push(`[clearing] LAN URL (current DHCP address): http://${ip}:${port}`);
  } else {
    lines.push('[clearing] no LAN address up — LAN access unavailable until wifi/ethernet returns');
  }
  return lines;
}

export interface IpDrift {
  drifted: boolean;
  from?: string;
  to?: string;
}

/** Drift = the machine moved from one LAN address to another. First boot and
 *  offline are not drift — only a real from→to change earns the breadcrumb. */
export function detectIpDrift(previous: string | null, current: string | null): IpDrift {
  if (previous && current && previous !== current) {
    return { drifted: true, from: previous, to: current };
  }
  return { drifted: false };
}
