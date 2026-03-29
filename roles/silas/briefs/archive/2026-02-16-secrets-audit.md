# Secrets Audit — Full Portfolio

**Card**: #50
**Date**: 2026-02-16
**From**: Silas (Architect)
**Status**: Complete — findings documented, recommendations pending Jeff review

---

## Executive Summary

**17 unique secrets** found across 4 projects, stored in **8 .env files** and **4 infrastructure files**. All are plaintext. No secrets manager in use. Several secrets are duplicated across projects. Two use weak defaults (`admin`, `admin123`).

**No secrets found committed to git** — .gitignore rules are correct across all projects. The risk is local-machine exposure and lack of rotation, not repository leaks.

---

## Inventory

### Tier 1 — External Service Credentials (rotate if compromised)

| # | Secret | Projects | Storage | Notes |
|---|--------|----------|---------|-------|
| 1 | **Anthropic API Key** (sk-ant-api03) | personal-site `.env`, `.env.bridge` | Plaintext .env | Duplicated in 2 locations — same key |
| 2 | **Slack Bot Token** (xoxb) | personal-site `.env`, messages `.env` | Plaintext .env | Duplicated in 2 locations — same key |
| 3 | **Twilio Auth Token** | personal-site `.env` | Plaintext .env | SMS capture feature |
| 4 | **Twilio Account SID** | personal-site `.env` | Plaintext .env | Paired with auth token |
| 5 | **Google OAuth Client ID** | personal-site `.env` | Plaintext .env | Photos Picker API |
| 6 | **Google OAuth Client Secret** | personal-site `.env` | Plaintext .env | Photos Picker API |
| 7 | **WordPress Webhook Secret** | personal-site `.env`, wordpress-blog `terraform.tfvars` + `tfstate` | Plaintext | Shared between projects, same value |

### Tier 2 — Internal Service Credentials (local network only)

| # | Secret | Projects | Storage | Notes |
|---|--------|----------|---------|-------|
| 8 | **Fuseki Admin Password** | personal-site `.env` | Plaintext .env | Value: `admin123` (weak) |
| 9 | **Grafana Admin Password** | shared-observability `.env` | Plaintext .env | Value: `admin` (default) |
| 10 | **MySQL Root Password** | wordpress-blog `main.tf`, `wp-config-persist.php`, `backup.sh`, `tfstate` | Hardcoded in 4+ files | `wordpress_root_password` |
| 11 | **MySQL User Password** | wordpress-blog `main.tf`, `wp-config-persist.php`, `backup.sh`, `tfstate` | Hardcoded in 4+ files | `wordpress_password` |
| 12 | **MySQL Exporter Password** | shared-observability `.env` | Plaintext .env | Same as MySQL user password |
| 13 | **Vikunja Tokens** (x3) | messages `.env` | Plaintext .env | Role-specific board access (Silas, Wren, Kade) |

### Tier 3 — Application Secrets (generated, local scope)

| # | Secret | Projects | Storage | Notes |
|---|--------|----------|---------|-------|
| 14 | **WordPress Auth Keys/Salts** (x8) | wordpress-blog `wp-config-persist.php` | Hardcoded PHP | Session/cookie crypto |
| 15 | **Session Secret** | personal-site `.env` | Plaintext .env | Express session signing |

### Tier 4 — PII / Configuration

| # | Secret | Projects | Storage | Notes |
|---|--------|----------|---------|-------|
| 16 | **Phone Number** | personal-site `.env` | Plaintext .env | SMS capture allowlist |
| 17 | **Local Network IPs** | personal-site `.env` | Plaintext .env | Gallery service endpoints |

---

## Structural Issues

### 1. Duplication
Two secrets are duplicated across projects:
- **Anthropic API Key**: `personal-site/.env` and `.env.bridge` (identical key)
- **Slack Bot Token**: `personal-site/.env` and `messages/.env` (identical key)

Duplication means rotating one requires finding and updating all copies.

### 2. Hardcoded in Infrastructure Code
WordPress blog has credentials hardcoded in:
- `terraform/main.tf` (MySQL creds as env vars)
- `content/wp-config-persist.php` (MySQL creds + auth keys)
- `backup.sh` (MySQL creds in shell command)
- `terraform/terraform.tfstate` and `.backup` (all secrets in state)

These aren't .env files — they're source/config files where secrets are baked in.

### 3. Weak Defaults
- Fuseki: `admin123`
- Grafana: `admin`
- MySQL: `wordpress_password` / `wordpress_root_password`

Acceptable for local dev, but these should be flagged if services ever become network-accessible.

### 4. No Rotation History
No evidence of any credential ever being rotated. No rotation dates tracked.

---

## What's Working

- **.gitignore is correct** across all projects — .env files are excluded
- **Source code is clean** — personal-site reads all secrets from `process.env`, no hardcoded values in TypeScript
- **Terraform marks sensitive vars** — `sensitive = true` on password variables
- **Template files exist** — `.env.example` files have placeholders, not real values

---

## Recommendations

### Phase 1 — Immediate (no new tools) — COMPLETED 2026-02-16

| Action | Status | What Changed |
|--------|--------|--------------|
| Add canonical-source comments to all .env files | Done | All 4 .env files + .env.bridge now have headers pointing to this audit and noting which secrets are shared/duplicated |
| Strengthen Fuseki default | Done | `admin123` → generated 20-char password. **Requires Fuseki container restart.** |
| Strengthen Grafana default | Done | `admin` → generated 20-char password. **Requires Grafana container restart.** |
| Extract WordPress hardcoded creds to .env | Done | Created `wordpress-blog/.env`, updated `main.tf` (4 new variables), `wp-config-persist.php` (getenv with fallbacks), `backup.sh` (sources .env). Added .env to .gitignore. Created `.env.example`. |
| Remove terraform.tfstate from repo | N/A | Already excluded by .gitignore, not tracked in git |
| Document all secrets in a single inventory | Done | This file |

**Container restart needed**: Fuseki and Grafana passwords changed in .env but running containers still use the old values. On next `docker-compose up` or `terraform apply`, they'll pick up the new passwords.

### Phase 2 — Backlog (card #48, assigned to Kade)

| Action | Effort | Impact |
|--------|--------|--------|
| Pre-commit hook: scan for secrets in staged files (e.g., detect-secrets) | 1 hr | Prevents accidental commits |
| Add secret rotation dates to inventory | 30 min | Audit trail |

### Phase 3 — Deferred (no current use case)

| Action | Trigger |
|--------|---------|
| Secrets manager (1Password CLI, Vault, or macOS Keychain) | When services go external |
| TLS for internal services | When services go external |
| Rotate all external credentials | When services go external |
