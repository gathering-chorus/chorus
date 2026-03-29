import { log } from './logger';

/** Map role names to their pod inbox directory names */
const ROLE_TO_POD_INBOX: Record<string, string> = {
  silas: 'to-architect',
  wren: 'to-pm',
  kade: 'to-engineer',
};

/** Valid role names for state file operations */
const VALID_ROLES = new Set(['silas', 'wren', 'kade']);

interface TokenCache {
  token: string;
  expiresAt: number;
}

/**
 * Low-level client for reading/writing briefs via the SOLID pod API.
 * All methods return success/failure — no thrown exceptions.
 */
export class PodBriefClient {
  private baseUrl: string;
  private agentId: string;
  private agentSecret: string;
  private tokenCache: TokenCache | null = null;

  constructor(baseUrl: string, agentId: string, agentSecret: string) {
    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.agentId = agentId;
    this.agentSecret = agentSecret;
  }

  /**
   * Get a service token, using cached value if still valid (55-min TTL).
   */
  async getServiceToken(): Promise<string | null> {
    // Return cached token if still valid (refresh 5 min before expiry)
    if (this.tokenCache && Date.now() < this.tokenCache.expiresAt - 5 * 60 * 1000) {
      return this.tokenCache.token;
    }

    try {
      const res = await fetch(`${this.baseUrl}/api/auth/service-token`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          agentId: this.agentId,
          agentSecret: this.agentSecret,
        }),
      });

      if (!res.ok) {
        log('warn', `Service token request failed: ${res.status} ${res.statusText}`, {
          event: 'pod_token_error',
        });
        return null;
      }

      const data = (await res.json()) as { token: string; expiresIn: number };
      this.tokenCache = {
        token: data.token,
        expiresAt: Date.now() + data.expiresIn * 1000,
      };
      log('info', 'Pod service token acquired', { event: 'pod_token_acquired' });
      return data.token;
    } catch (err) {
      log('warn', `Service token request error: ${err instanceof Error ? err.message : String(err)}`, {
        event: 'pod_token_error',
      });
      return null;
    }
  }

  /**
   * Write a brief to the pod. Returns true on success.
   */
  async writeBriefToPod(toRole: string, filename: string, content: string): Promise<boolean> {
    const inbox = ROLE_TO_POD_INBOX[toRole];
    if (!inbox) {
      log('warn', `Unknown role for pod write: ${toRole}`, { event: 'pod_write_error' });
      return false;
    }

    const token = await this.getServiceToken();
    if (!token) return false;

    try {
      const url = `${this.baseUrl}/api/service/pods/jeff/coordination/briefs/${inbox}/${filename}`;
      const res = await fetch(url, {
        method: 'PUT',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'text/markdown',
        },
        body: content,
      });

      if (!res.ok) {
        log('warn', `Pod write failed: ${res.status} ${res.statusText}`, {
          event: 'pod_write_error',
          toRole,
          filename,
        });
        return false;
      }

      log('info', `Brief written to pod: ${inbox}/${filename}`, {
        event: 'pod_write_success',
        toRole,
        filename,
      });
      return true;
    } catch (err) {
      log('warn', `Pod write error: ${err instanceof Error ? err.message : String(err)}`, {
        event: 'pod_write_error',
        toRole,
        filename,
      });
      return false;
    }
  }

  /**
   * Read a brief from the pod. Returns content or null on failure.
   */
  async readBriefFromPod(toRole: string, filename: string): Promise<string | null> {
    const inbox = ROLE_TO_POD_INBOX[toRole];
    if (!inbox) {
      log('warn', `Unknown role for pod read: ${toRole}`, { event: 'pod_read_error' });
      return null;
    }

    const token = await this.getServiceToken();
    if (!token) return null;

    try {
      const url = `${this.baseUrl}/api/service/pods/jeff/coordination/briefs/${inbox}/${filename}`;
      const res = await fetch(url, {
        method: 'GET',
        headers: { 'Authorization': `Bearer ${token}` },
      });

      if (!res.ok) {
        log('warn', `Pod read failed: ${res.status} ${res.statusText}`, {
          event: 'pod_read_error',
          toRole,
          filename,
        });
        return null;
      }

      const text = await res.text();
      return text;
    } catch (err) {
      log('warn', `Pod read error: ${err instanceof Error ? err.message : String(err)}`, {
        event: 'pod_read_error',
        toRole,
        filename,
      });
      return null;
    }
  }

  /**
   * Write a state file to the pod. Returns true on success.
   */
  async writeStateToPod(role: string, filename: string, content: string): Promise<boolean> {
    if (!VALID_ROLES.has(role)) {
      log('warn', `Unknown role for state write: ${role}`, { event: 'pod_state_write_error' });
      return false;
    }

    const token = await this.getServiceToken();
    if (!token) return false;

    try {
      const url = `${this.baseUrl}/api/service/pods/jeff/coordination/state/${role}/${filename}`;
      const res = await fetch(url, {
        method: 'PUT',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'text/markdown',
        },
        body: content,
      });

      if (!res.ok) {
        log('warn', `Pod state write failed: ${res.status} ${res.statusText}`, {
          event: 'pod_state_write_error',
          role,
          filename,
        });
        return false;
      }

      log('info', `State written to pod: ${role}/${filename}`, {
        event: 'pod_state_write_success',
        role,
        filename,
      });
      return true;
    } catch (err) {
      log('warn', `Pod state write error: ${err instanceof Error ? err.message : String(err)}`, {
        event: 'pod_state_write_error',
        role,
        filename,
      });
      return false;
    }
  }

  /**
   * Read a state file from the pod. Returns content or null on failure.
   */
  async readStateFromPod(role: string, filename: string): Promise<string | null> {
    if (!VALID_ROLES.has(role)) {
      log('warn', `Unknown role for state read: ${role}`, { event: 'pod_state_read_error' });
      return null;
    }

    const token = await this.getServiceToken();
    if (!token) return null;

    try {
      const url = `${this.baseUrl}/api/service/pods/jeff/coordination/state/${role}/${filename}`;
      const res = await fetch(url, {
        method: 'GET',
        headers: { 'Authorization': `Bearer ${token}` },
      });

      if (!res.ok) {
        log('warn', `Pod state read failed: ${res.status} ${res.statusText}`, {
          event: 'pod_state_read_error',
          role,
          filename,
        });
        return null;
      }

      return await res.text();
    } catch (err) {
      log('warn', `Pod state read error: ${err instanceof Error ? err.message : String(err)}`, {
        event: 'pod_state_read_error',
        role,
        filename,
      });
      return null;
    }
  }
}
