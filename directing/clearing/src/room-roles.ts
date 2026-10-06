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
