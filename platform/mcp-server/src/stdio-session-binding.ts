import { request } from 'node:http';
import { homedir } from 'node:os';
import { join } from 'node:path';

export type ProfileBinding = { session_id: string; principal: string; role: string };
export type LookupProfileBinding = (profile: string, role: string) => Promise<ProfileBinding>;

export function localProfileBinding(socket = process.env.CHORUS_AGENT_SOCKET || join(process.env.CHORUS_AGENT_STATE_DIR || join(homedir(), '.chorus'), 'run/chorus-agent.sock')): LookupProfileBinding {
  return (profile, role) => new Promise((resolve, reject) => {
    const body = JSON.stringify({ profile, role });
    const req = request({ socketPath: socket, path: '/v1/profile-binding', method: 'POST', headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) } }, res => {
      let data = '';
      res.setEncoding('utf8');
      res.on('data', chunk => { data += chunk; if (data.length > 16384) req.destroy(new Error('profile-binding-response-too-large')); });
      res.on('error', reject);
      res.on('end', () => {
        if (res.statusCode !== 200) { reject(new Error('profile-binding-unavailable')); return; }
        try {
          const value = JSON.parse(data) as ProfileBinding;
          if (!value || typeof value.session_id !== 'string' || !/^[A-Za-z0-9_-]{1,200}$/.test(value.session_id)
            || typeof value.principal !== 'string' || !value.principal || typeof value.role !== 'string') throw new Error('invalid binding');
          resolve({ session_id: value.session_id, principal: value.principal, role: value.role });
        } catch { reject(new Error('profile-binding-response-invalid')); }
      });
    });
    req.setTimeout(5000, () => req.destroy(new Error('profile-binding-timeout')));
    req.on('error', reject);
    req.end(body);
  });
}

/** An MCP bridge belongs to one conversation. A replacement profile session
 * requires restarting the bridge, never silent routing to a new conversation. */
export function pinProfileBinding(profile: string, role: string, lookup: LookupProfileBinding = localProfileBinding(), sessionId?: string): () => Promise<ProfileBinding> {
  if (!profile || !role) throw new Error('profile-binding-profile-and-role-required');
  let pinned: ProfileBinding | undefined;
  return async () => {
    const current = await lookup(profile, role);
    if (current.role !== role || (sessionId && current.session_id !== sessionId)) throw new Error('profile-binding-role-or-session-mismatch');
    if (pinned && (current.session_id !== pinned.session_id || current.principal !== pinned.principal || current.role !== pinned.role)) {
      throw new Error('profile-binding-changed-restart-bridge');
    }
    pinned = { ...current };
    return { ...current };
  };
}
