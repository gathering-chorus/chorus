# Brief: Deploy stability improvements — three changes shipped

**From:** Silas (Architect)
**To:** Wren (Product Manager)
**Date:** 2026-03-02

## What happened

Jeff noticed the app going up and down. Root cause: 8 deploys in 2.5 hours from Kade's harvest pipeline work, plus roles calling `restart` independently. Each deploy also bounced Fuseki unnecessarily — adding ~90s downtime per cycle.

## Three changes shipped (all in app-state.sh, live immediately)

### 1. Deploy cooldown (3 minutes)
After a successful deploy or restart, further attempts within 180s are rejected with a log message. Prevents pile-on from multiple roles pushing within minutes of each other. Override: `DEPLOY_FORCE=1`.

### 2. Healthy-skip guard on restart
If the container is already healthy, `restart` skips with a log message. Prevents no-op restarts that still cycle the container.

### 3. Scoped compose up — app only
`docker compose up -d` → `docker compose up -d app`. Deploys and rollbacks now only recreate the app container. Fuseki and WebVOWL stay running. Cold start (no containers) still brings up everything. Deploy downtime drops from 30-90s to ~5-10s.

## Impact on roles

- **Kade**: Deploy batching brief also sent. Push once when a batch is done, not per-commit. The cooldown guard backstops this.
- **Wren**: No process change needed. If you push view/CSS changes, no deploy fires (existing behavior). If you push src/ changes, the cooldown and SHA guards protect against redundant deploys.
- **Jeff**: Fewer interruptions. App stays up during normal development flow.

## Also shipped

- **alert-notifier.py**: Switched from `osascript` to `terminal-notifier` for macOS notifications. Eliminates the TCC "2.1.63 would like to access data" modal that was interrupting Jeff 4-6x/day.
