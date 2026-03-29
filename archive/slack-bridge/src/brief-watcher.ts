import * as fs from 'fs';
import * as path from 'path';
import { RoleConfig } from './config';
import { ChannelMonitor } from './channel-monitor';
import { ResponsePoster } from './response-poster';
import { listBriefs } from './pod-brief-io';
import { log } from './logger';

/**
 * Watches briefs/ directories for new files and posts a notification
 * to the recipient role's Slack channel. Eliminates the need for
 * Jeff to relay "check your briefs" between sessions.
 *
 * State is persisted to disk so we don't re-notify after restarts.
 */
export class BriefWatcher {
  private roles: RoleConfig[];
  private monitor: ChannelMonitor;
  private poster: ResponsePoster;
  private seenFiles: Set<string>;
  private statePath: string;

  constructor(
    roles: RoleConfig[],
    monitor: ChannelMonitor,
    poster: ResponsePoster,
    dataDir: string
  ) {
    this.roles = roles;
    this.monitor = monitor;
    this.poster = poster;
    this.statePath = path.join(dataDir, 'brief-watcher-seen.json');
    this.seenFiles = this.loadState();
  }

  /**
   * Scan all briefs directories for new files. Called each poll cycle.
   * Posts a notification to the role's Slack channel for any new brief.
   */
  async scan(): Promise<void> {
    for (const role of this.roles) {
      try {
        const files = listBriefs(role);

        for (const file of files) {
          const key = `${role.name}:${file}`;
          if (this.seenFiles.has(key)) continue;

          // New brief found — notify the role
          const fullPath = path.join(role.briefsPath, file);
          const stat = fs.statSync(fullPath);

          // Only notify for briefs less than 24h old (skip historical on first run)
          const ageHours = (Date.now() - stat.mtimeMs) / (1000 * 60 * 60);
          if (ageHours > 24) {
            this.seenFiles.add(key);
            continue;
          }

          await this.notifyRole(role, file);
          this.seenFiles.add(key);
        }
      } catch (err) {
        log('warn', `Brief watcher error for ${role.name}: ${err}`);
      }
    }

    this.saveState();
  }

  private async notifyRole(role: RoleConfig, filename: string): Promise<void> {
    const channelId = this.monitor.getChannelId(role.channel);
    if (!channelId) {
      log('warn', `No channel ID for ${role.channel}, skipping brief notification`);
      return;
    }

    const shortName = humanizeBriefName(filename);
    const message = `:page_facing_up: New brief in your inbox: **${shortName}** — check \`briefs/${filename}\``;

    try {
      await this.poster.postSystem(channelId, message);
      log('info', `Brief notification sent to ${role.name}: ${filename}`);
    } catch (err) {
      log('error', `Failed to notify ${role.name} about brief ${filename}: ${err}`);
    }
  }

  private loadState(): Set<string> {
    try {
      if (fs.existsSync(this.statePath)) {
        const data = JSON.parse(fs.readFileSync(this.statePath, 'utf-8'));
        return new Set(data);
      }
    } catch (err) {
      log('warn', `Could not load brief watcher state: ${err}`);
    }
    return new Set();
  }

  private saveState(): void {
    try {
      const dir = path.dirname(this.statePath);
      if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(this.statePath, JSON.stringify([...this.seenFiles]), 'utf-8');
    } catch (err) {
      log('warn', `Could not save brief watcher state: ${err}`);
    }
  }
}

function humanizeBriefName(filename: string): string {
  return filename
    .replace(/^\d{4}-\d{2}-\d{2}-/, '')
    .replace(/\.md$/, '')
    .replace(/-\d{4}$/, '')
    .replace(/-/g, ' ')
    .replace(/^./, c => c.toUpperCase());
}
