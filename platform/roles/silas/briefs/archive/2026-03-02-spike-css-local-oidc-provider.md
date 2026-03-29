# Spike: Self-Hosted Community Solid Server (CSS) as Local OIDC Provider

**Date**: 2026-03-02
**Author**: Silas (Architect)
**Status**: Research complete
**Card**: spike (uncarded, routed from Jeff)

---

## Problem

The app currently authenticates via solidcommunity.net using Solid-OIDC. Pivot (solidcommunity.net) is slow -- 3.5s OIDC discovery timeouts, login loops, and the PKCE token exchange is unreliable (the callback handler has a comment: "Standard OIDC flow -- Currently broken due to in-memory PKCE storage"). The current workaround ("Pivot callback") maps the issuer URL directly to Jeff's WebID without completing a full OIDC token exchange. This is fragile and only works because there is one user.

## Findings

### 1. CSS Docker Image

- **Image**: `solidproject/community-server` on Docker Hub
- **Latest version**: v7.1.8 (released 2026-01-14)
- **npm package**: `@solid/community-server@7.1.8`
- **Default port**: 3000
- **Env var prefix**: `CSS_` (e.g., `CSS_PORT`, `CSS_BASE_URL`, `CSS_CONFIG`, `CSS_LOGGING_LEVEL`)

### 2. CSS as OIDC Provider

CSS is a **full Solid-OIDC provider out of the box**. It uses `node-oidc-provider` internally and implements the Solid-OIDC spec. Key behaviors:

- Accounts, pods, and WebIDs are created through a JSON API at `/.account/`
- When a pod is created, CSS auto-generates a WebID with `solid:oidcIssuer` pointing to itself
- Dynamic client registration is built-in (the `@inrupt/solid-client-authn-node` library uses this)
- No external IdP, no Keycloak, no Dex needed -- CSS IS the IdP

### 3. Minimal Docker Compose Service

```yaml
  css:
    container_name: jeff-bridwell-personal-site-css
    image: solidproject/community-server:7.1.8
    ports:
      - "127.0.0.1:3001:3000"    # Localhost only; 3000 is taken by Express app
    environment:
      - CSS_BASE_URL=https://localhost:3001/
      - CSS_LOGGING_LEVEL=info
    command: ["-c", "@css:config/file.json", "-f", "/data"]
    volumes:
      - css-data:/data
    networks:
      - app-network
    healthcheck:
      test: ["CMD", "wget", "--no-verbose", "--tries=1", "--spider", "http://127.0.0.1:3000/"]
      interval: 30s
      timeout: 5s
      start_period: 15s
      retries: 3
    restart: unless-stopped
```

Add to volumes section:
```yaml
  css-data:
    name: jeff-bridwell-personal-site-css-data
```

**Port choice**: Express app owns 3000. CSS listens internally on 3000 but maps to host port 3001. The `CSS_BASE_URL` must match what the browser will see (https://localhost:3001/).

**TLS note**: CSS_BASE_URL should use https if the Express app redirects OIDC flows over HTTPS. CSS itself doesn't do TLS -- you'd either:
- (a) Put it behind the same TLS proxy as the app, or
- (b) Use HTTP internally and let Docker networking handle it (simpler for local dev, but the browser redirect needs HTTPS for Solid-OIDC to work properly)

Best approach: add CSS to the existing reverse-proxy/TLS setup, or have the Express app proxy OIDC discovery requests to CSS over HTTP internally.

### 4. User/Pod Seeding (Post-Deploy Bootstrap)

CSS exposes a JSON API for account management. A seed script would run once after first deploy:

```bash
#!/bin/bash
# seed-css-account.sh -- run once after first CSS container start
CSS_URL="http://localhost:3001"

# Step 1: Create account (returns auth cookie)
COOKIE=$(curl -s -c - -X POST "$CSS_URL/.account/" | grep css-account | awk '{print $NF}')

# Step 2: Add password login
curl -s -b "css-account=$COOKIE" -X POST "$CSS_URL/.account/password/" \
  -H "Content-Type: application/json" \
  -d '{"email":"jeff@jeffbridwell.com","password":"<secure>"}'

# Step 3: Create pod + auto-generate WebID
curl -s -b "css-account=$COOKIE" -X POST "$CSS_URL/.account/pod/" \
  -H "Content-Type: application/json" \
  -d '{"name":"jeff"}'

# Result: WebID at http://localhost:3001/jeff/profile/card#me
# with solid:oidcIssuer <http://localhost:3001/>
```

The generated WebID would be: `https://localhost:3001/jeff/profile/card#me`

### 5. App Changes Required

The blast radius is moderate. Here's what would change:

#### a. Login form (`views/index.ejs`, line 148-163)
Add a third provider option (or replace solidcommunity.net):
```html
<label class="provider-option">
    <input type="radio" name="provider" value="https://localhost:3001" checked>
    <span>Local (CSS)</span>
</label>
```

#### b. Authorized users (`src/config/authorized-users.ts`)
Add the new local WebID:
```typescript
{
  webId: 'https://localhost:3001/jeff/profile/card#me',
  name: 'Jeff Bridwell',
  role: 'admin',
  notes: 'Local CSS WebID'
}
```

#### c. Callback handler (`src/handlers/callback.handler.ts`, line 64-67)
Add issuer-to-WebID mapping for local CSS:
```typescript
const issuerToWebId: Record<string, string> = {
  'https://solidcommunity.net/': 'https://jeffbridwell.solidcommunity.net/profile/card#me',
  'https://solidcommunity.net': 'https://jeffbridwell.solidcommunity.net/profile/card#me',
  'https://localhost:3001/': 'https://localhost:3001/jeff/profile/card#me',
  'https://localhost:3001': 'https://localhost:3001/jeff/profile/card#me',
};
```

**However**: With a local CSS, the standard OIDC flow (PKCE) should actually work because there's no network latency or timeout. The Pivot workaround may become unnecessary. The `@inrupt/solid-client-authn-node` library's PKCE storage is now backed by SQLite (`src/services/oidc-storage.ts`), and with CSS responding in <10ms instead of 3.5s, the full OIDC flow should complete reliably.

#### d. CSP headers (`src/app.ts`, line 416-460)
Add `https://localhost:3001` to `connectSrc`, `frameSrc`, and `formAction` arrays. Or better: make these configurable via env var.

#### e. CORS middleware (`src/middleware/cors.middleware.ts`)
May need `https://localhost:3001` in allowed origins.

#### f. Environment variable (new)
Add `SOLID_OIDC_ISSUER` env var to make the provider configurable without code changes:
```
SOLID_OIDC_ISSUER=https://localhost:3001
```

Currently the provider is hardcoded in the view template and the callback handler. This should be extracted.

### 6. What Does NOT Change

- Pod data storage (Fuseki) -- unaffected. CSS is only the IdP, not the data store.
- `@inrupt/solid-client` usage for pod operations -- unaffected.
- The WebID concept and authorized-users pattern -- same, just a different WebID URI.
- Existing test users and agent WebIDs -- those use `localhost:3000` URIs, unrelated.

## Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Port conflict (CSS wants 3000, app has 3000) | Low | Map CSS to host 3001, internal 3000 |
| TLS mismatch (browser HTTPS, CSS HTTP) | Medium | Proxy CSS through existing TLS setup or use HTTP for local dev |
| CSS_BASE_URL must match browser URL exactly | Medium | Get this right in docker-compose; mismatch breaks OIDC discovery |
| CSS data volume loss = locked out | Medium | Seed script is idempotent; CSS data volume persisted |
| WebID URI migration (old solidcommunity.net references) | Low | Keep both in authorized-users; transition gradually |
| Memory footprint -- CSS is a Node.js server | Low | ~80-120MB RSS; M1 16GB has headroom |
| Breaking OIDC flow on LAN access (192.168.x.x vs localhost) | Medium | CSS_BASE_URL must account for all access patterns; may need multiple or wildcard |

## Recommendation

**Do it.** The migration is straightforward and the payoff is significant:

1. **Login goes from 3.5s+ to <100ms** -- no external network dependency
2. **The broken PKCE flow should work** -- latency was the root cause, and OIDC storage is already SQLite-backed
3. **No external dependency for auth** -- solidcommunity.net going down no longer blocks Jeff
4. **Infrastructure coherence** -- the IdP runs alongside the data store, as it should for a personal site
5. **Minimal blast radius** -- 5-6 files change, no architectural shift

**Suggested approach**:
1. Card it (P2, Kade builds, Silas reviews compose changes)
2. Add CSS to docker-compose with file-based persistence
3. Write seed script for Jeff's account/pod
4. Extract `SOLID_OIDC_ISSUER` env var
5. Update views + callback + CSP + authorized-users
6. Test: login via local CSS, verify session, verify existing solidcommunity.net still works as fallback
7. Once stable, make local CSS the default provider

**Estimated effort**: Small-to-medium. Half a session for compose + seed, half a session for app changes + testing.

---

## Sources
- [CSS GitHub Repository](https://github.com/CommunitySolidServer/CommunitySolidServer) -- v7.1.8
- [CSS Getting Started Tutorial](https://github.com/CommunitySolidServer/tutorials/blob/main/getting-started.md)
- [CSS v7.x Starting the Server](https://communitysolidserver.github.io/CommunitySolidServer/7.x/usage/starting-server/)
- [CSS v7.x JSON API for Account Management](https://communitysolidserver.github.io/CommunitySolidServer/7.x/usage/account/json-api/)
- [CSS Identity Config README](https://github.com/CommunitySolidServer/CommunitySolidServer/blob/main/config/identity/README.md)
- [CSS Docker Hub Image](https://hub.docker.com/r/solidproject/community-server)
- [Solid Forum: Self-hosted CSS Registration Issues](https://forum.solidproject.org/t/cant-register-to-selfhosted-pod-using-css/10160)
- [CSS v5.x Identity Provider Docs](https://communitysolidserver.github.io/CommunitySolidServer/5.x/usage/identity-provider/)
- [npm @solid/community-server](https://www.npmjs.com/package/@solid/community-server)
