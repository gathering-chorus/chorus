// @domain: messages
// #4649 — which of Jeff's messages did not reach which role. The page marks them
// "not delivered to <role>"; the server keeps the list so a reload or a second phone
// shows the same marks. In memory and bounded: a restart clears it, and the hand-off
// error stays in clearing-v2.log either way.
export class UndeliveredLog {
  private readonly byId = new Map<string, string[]>();

  constructor(private readonly limit = 200) {}

  record(id: string, role: string): string[] {
    const roles = this.byId.get(id) ?? [];
    if (!roles.includes(role)) roles.push(role);
    this.byId.delete(id);
    this.byId.set(id, roles);
    while (this.byId.size > this.limit) {
      const oldest = this.byId.keys().next().value as string;
      this.byId.delete(oldest);
    }
    return [...roles];
  }

  all(): Record<string, string[]> {
    return Object.fromEntries([...this.byId].map(([id, roles]) => [id, [...roles]]));
  }
}
