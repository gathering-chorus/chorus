// @domain: messages
/**
 * #4432 — who is in the room besides Jeff: the agent roles the tile poller last
 * read from /api/chorus/context/roles (the roles door's rows). One list for the
 * tiles, the @mention targets and the role-to-role filter, never three names
 * typed into each. Empty until the API has answered once: nothing is guessed.
 */
let roles: string[] = [];

export function setRoomRoles(next: string[]): void {
  roles = next.filter((r) => r && r !== 'jeff');
}

export function roomRoles(): string[] {
  return [...roles];
}

export function isRoomRole(name: string): boolean {
  return roles.includes(name.toLowerCase());
}

/**
 * #4445 — the roles whose replies the session tailer reads from their Claude
 * Code transcripts. Any other room role (Abby runs Gemini, which writes no Claude
 * transcript) reaches the Clearing through its reply notes on the relay instead.
 * One list, read by both: the tailer tails these, the room skips their reply
 * notes so nothing renders twice.
 */
export const TAILED_ROLES = ['wren', 'silas', 'kade'] as const;

export function isTailedRole(name: string): boolean {
  return (TAILED_ROLES as readonly string[]).includes(name);
}
