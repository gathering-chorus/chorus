# Spike: External Security Scanning for Home-Hosted Web Application

**Date**: 2026-02-19
**Author**: Silas (Architect)
**Status**: Complete
**Time-box**: 1 session (research + synthesis)

---

## Question

What are the viable options for running automated security scans FROM the cloud AGAINST a home-hosted Express/Node.js app on a Mac mini? What runs serverless, what does it cost, and what are the gotchas?

## Context

The Gathering app runs on a Mac mini at 192.168.86.36:3000. It is not currently internet-reachable. Any external scanner needs either (a) the app exposed to the internet via tunnel, or (b) a VPN back into the home network. The goal is weekly automated scans at minimal cost, with findings routed to S3 or a notification channel.

---

## Network Reachability: ALREADY SOLVED

The app is **already internet-reachable** at `https://lightlifeurbangardens.com` via Cloudflare Tunnel.

| Component | Status |
|-----------|--------|
| Cloudflare Tunnel | Active (tunnel ID `57f35c2d-...`, PID 57503) |
| Domain | `lightlifeurbangardens.com` |
| Protocol | Outbound-only QUIC tunnel — no inbound ports open |
| Security | DDoS protection at Cloudflare edge, home IP never exposed |
| Health endpoint | `https://lightlifeurbangardens.com/health` |
| Config | `jeff-bridwell-personal-site/.cloudflared/config.yml` |
| Docs | `jeff-bridwell-personal-site/docs/PUBLIC_ACCESS.md` |
| ADR | 2026-02-07 — Cloudflare Tunnel chosen over open ports |

**Health note**: Logs show intermittent QUIC routing errors to Cloudflare edge, auto-recovering. Functional but occasionally flaky.

No setup needed — all scanners can target `https://lightlifeurbangardens.com` immediately.

---

## Option 1: OWASP ZAP

### What It Is
The standard open-source DAST (Dynamic Application Security Testing) tool. Spider + passive scan + active scan. The most comprehensive option.

### Serverless Feasibility: YES (Fargate or CodeBuild, NOT Lambda)

ZAP is a Java application requiring 1-2 GB RAM. The baseline scan (spider + passive) runs 2-5 minutes. A full active scan can run 30-60+ minutes. Lambda's 15-minute timeout and 10 GB memory limit make it marginal for full scans.

**Best deployment pattern**: AWS published a [reference architecture](https://aws.amazon.com/blogs/architecture/modernize-your-penetration-testing-architecture-on-aws-fargate/) using:
- EventBridge (cron trigger) -> Lambda (build task definition) -> ECS Fargate (run ZAP container) -> S3 (store results)
- Alternative: **AWS CodeBuild** (simpler). One `buildspec.yml`, pay per build-minute. No ECS cluster management.

### Container Image
```
docker pull ghcr.io/zaproxy/zaproxy:stable
```
Stable image, updated monthly. Weekly and nightly variants available.

### Scan Types
| Scan | Duration | What It Does |
|------|----------|--------------|
| Baseline | 2-5 min | Spider (1 min) + passive scan. No attacks. Safe for production. |
| Full | 30-60+ min | Spider + passive + active attacks. Tests for SQLi, XSS, etc. |
| API | 5-15 min | OpenAPI/Swagger-driven scan. Good fit — we have `/api-docs`. |

### Resource Requirements
- **Baseline**: 0.5 vCPU, 1 GB RAM (Fargate minimum task)
- **Full scan**: 1 vCPU, 2 GB RAM recommended
- **Disk**: 20 GB ephemeral (Fargate default, free)

### Cost Estimate (Weekly Baseline Scan via Fargate)
| Component | Calculation | Monthly Cost |
|-----------|-------------|-------------|
| Fargate (0.5 vCPU, 1GB, 5 min/week) | 20 min/month * $0.014/hr | ~$0.005 |
| EventBridge rule | Free tier | $0.00 |
| Lambda orchestrator | Free tier (1M invocations/mo) | $0.00 |
| S3 results storage | Negligible | ~$0.01 |
| **Total** | | **< $0.05/month** |

### Cost Estimate (Weekly Full Scan via CodeBuild)
| Component | Calculation | Monthly Cost |
|-----------|-------------|-------------|
| CodeBuild (general1.small, ~45 min/week) | 180 min/month * $0.005/min | ~$0.90 |
| S3 results | Negligible | ~$0.01 |
| **Total** | | **< $1.00/month** |

### Gotchas
- ZAP's active scan WILL send attack payloads (SQLi, XSS) to your app. Run during low-usage windows.
- Java memory: set `-Xmx1g` explicitly or ZAP will try to grab 25% of available memory.
- First run downloads add-ons; subsequent runs are faster.
- The ZAP Docker image is ~1.2 GB. ECR storage cost is negligible but first pull is slow.

---

## Option 2: Nuclei (ProjectDiscovery)

### What It Is
A fast, template-based vulnerability scanner. 8,000+ community templates covering CVEs, misconfigurations, default credentials, exposed panels, tech detection. Written in Go — single binary, low resource usage.

### Serverless Feasibility: YES (Lambda-native)

Nuclei is the best Lambda fit of all options. Single Go binary, low memory, fast execution. A single-target scan with all templates completes in ~2 minutes.

**Proven deployment**: [Nuclear Pond](https://github.com/DevSecOpsDocs/nuclearpond) — open-source Terraform module that deploys Nuclei as a Lambda function with S3 results storage and Athena querying.

Architecture:
- Terraform deploys Lambda + S3 + Athena + Glue
- CLI tool invokes Lambda with target + template args
- Results stored as JSON in S3, queryable via Athena
- EventBridge cron can trigger weekly scans

### Container/Binary
```bash
# Go binary (for Lambda layer)
go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest

# Docker
docker pull projectdiscovery/nuclei:latest
```

### Resource Requirements
- **Single target, all templates**: ~512 MB RAM, 2-3 minutes
- **Single target, focused templates**: ~256 MB RAM, 30-60 seconds
- Well within Lambda's 15-minute / 10 GB limits

### Cost Estimate (Weekly Scan via Lambda)
| Component | Calculation | Monthly Cost |
|-----------|-------------|-------------|
| Lambda (512 MB, 3 min, 4x/month) | 12 min * 0.5 GB = 6 GB-sec * $0.0000166667 | ~$0.0001 |
| Lambda requests | 4 requests | Free tier |
| S3 results | < 1 MB/month | ~$0.00 |
| Athena queries (optional) | Free tier (5 TB/month) | $0.00 |
| **Total** | | **< $0.01/month** |

Effectively free. Well within Lambda free tier (400,000 GB-seconds/month).

### Gotchas
- Templates update frequently. Build a CI step or cron to refresh the Lambda layer/image periodically.
- Some templates send active exploit payloads. Use `-severity low,medium,high` or `-type http` to control scope.
- Nuclear Pond hasn't been updated recently (last commit 2023). The Terraform module works but may need pinning.
- Nuclei does NOT spider your app — it tests known vulnerability patterns against URLs you give it. Pair with a crawler or feed it your sitemap.

---

## Option 3: Nikto

### What It Is
A Perl-based web server scanner. Tests for 7,000+ dangerous files, outdated server software, misconfigurations. Lightweight and fast, but narrower scope than ZAP or Nuclei.

### Serverless Feasibility: MARGINAL (Fargate yes, Lambda difficult)

Nikto is Perl-based with system dependencies (OpenSSL, LibWhisker). Packaging for Lambda is painful. Containerized via Fargate or CodeBuild is straightforward.

### Container Image
```bash
# Official Docker support
docker pull ghcr.io/sullo/nikto
# Or build from repo
git clone https://github.com/sullo/nikto && cd nikto && docker build -t nikto .
```

### Resource Requirements
- 0.25 vCPU, 512 MB RAM
- Scan of single target: 3-10 minutes

### Cost Estimate (Weekly via CodeBuild)
| Component | Calculation | Monthly Cost |
|-----------|-------------|-------------|
| CodeBuild (general1.small, ~10 min/week) | 40 min/month * $0.005/min | ~$0.20 |
| S3 results | Negligible | ~$0.01 |
| **Total** | | **< $0.25/month** |

### Gotchas
- Nikto is a legacy tool. Still maintained but development pace is slow.
- Noisy — generates many informational findings that need filtering.
- No template ecosystem like Nuclei. What it ships with is what you get.
- Overlaps significantly with ZAP's baseline scan. Adds limited value if you already run ZAP.

---

## Option 4: AWS-Native Security Services

### AWS Inspector
- **Scans external targets?** NO. Only scans EC2 instances, ECR container images, and Lambda functions within your AWS account.
- **Verdict**: Not applicable.

### AWS Security Hub
- **Scans external targets?** NO. Aggregates findings from AWS services (Inspector, GuardDuty, Macie, etc.). Cannot initiate scans against external URLs.
- **Verdict**: Not applicable. Could be used as a findings aggregator if you import scan results via the ASFF format, but that is overengineered for this use case.

### AWS GuardDuty
- **Scans external targets?** NO. Monitors AWS account activity (CloudTrail, VPC Flow Logs, DNS logs) for threats. Entirely inward-looking.
- **Verdict**: Not applicable.

### Summary
**None of the AWS-native security services can scan a non-AWS target.** They are designed to monitor AWS resources, not external web applications. Do not pursue this path.

---

## Option 5: Lightweight / Header-Level Scanners

### testssl.sh

| Attribute | Value |
|-----------|-------|
| What | Bash script testing TLS/SSL cipher support, protocols, vulnerabilities (POODLE, Heartbleed, etc.) |
| Lambda feasible? | YES — with custom Lambda layer containing bash + OpenSSL. Or Fargate trivially. |
| Docker | `docker pull ghcr.io/testssl/testssl.sh` |
| Scan time | 1-3 minutes per host |
| Cost (Lambda, weekly) | Effectively $0.00 (well within free tier) |
| Gotcha | Only tests TLS configuration. Does not test application-level vulnerabilities. |

### Mozilla HTTP Observatory

| Attribute | Value |
|-----------|-------|
| What | Tests HTTP security headers (CSP, HSTS, X-Frame-Options, etc.) and basic configuration |
| Lambda feasible? | YES — it's an API call. No deployment needed. |
| API endpoint | `POST https://http-observatory.security.mozilla.org/api/v1/analyze?host=yourdomain.com` |
| Rate limit | 1 scan per 3 minutes per host. Cached results for 24 hours. |
| Authentication | None required |
| Cost | $0.00 (free public API) |
| Gotcha | Target MUST be publicly accessible on port 443. Mozilla's servers do the scanning. Cannot reach a private IP. Requires the Cloudflare Tunnel to be set up first. |

### SecurityHeaders.com

| Attribute | Value |
|-----------|-------|
| What | Analyzes HTTP response headers for security best practices |
| API available? | Yes. Starts at $2.99/month for API key. Website is free. |
| Lambda feasible? | YES — API call. |
| Cost | $0.00 (website) or $2.99/month (API) |
| Gotcha | Same as Observatory — target must be publicly accessible. Overlaps heavily with Mozilla Observatory. |

### nmap (port scanning)

| Attribute | Value |
|-----------|-------|
| What | Network port scanner, service detection, OS fingerprinting |
| Lambda feasible? | YES — with static binary in Lambda layer. Auth0 [published a working approach](https://auth0.com/blog/aws-increases-security-scan-freedom/). |
| Fargate | Also works. AWS [reference architecture](https://aws.amazon.com/blogs/architecture/modernize-your-penetration-testing-architecture-on-aws-fargate/) uses `securecodebox/nmap` container. |
| Scan time | 30 seconds - 5 minutes for a single host |
| Cost (Lambda, weekly) | Effectively $0.00 |
| Gotcha | Only scans network layer (open ports, services). Not a web vulnerability scanner. Useful as a complement, not a replacement. Through a Cloudflare Tunnel, nmap only sees port 443 — it cannot scan the actual host's ports. Use Tailscale instead if port scanning is the goal. |

---

## Cost Summary (All Viable Options, Weekly Against Single Target)

| Tool | Deployment | Monthly Cost | What It Tests |
|------|-----------|-------------|---------------|
| **Nuclei** (Lambda) | Nuclear Pond / Terraform | < $0.01 | CVEs, misconfigs, exposed panels, tech detection |
| **ZAP Baseline** (Fargate) | EventBridge + Lambda + ECS | < $0.05 | Spider + passive scan (headers, cookies, info disclosure) |
| **ZAP Full** (CodeBuild) | CodeBuild project | < $1.00 | Active attacks (SQLi, XSS, CSRF, etc.) |
| **Nikto** (CodeBuild) | CodeBuild project | < $0.25 | Server misconfigs, dangerous files, outdated software |
| **testssl.sh** (Lambda) | Lambda + layer | < $0.01 | TLS/SSL configuration |
| **Mozilla Observatory** (API) | Lambda + API call | $0.00 | HTTP security headers |
| **SecurityHeaders.com** (API) | Lambda + API call | $0.00-$2.99 | HTTP security headers (overlaps Observatory) |
| **nmap** (Lambda) | Lambda + static binary | < $0.01 | Open ports, services (limited through tunnel) |
| **Cloudflare Tunnel** | `cloudflared` on Mac mini | $0.00 | N/A — prerequisite for all above |

**Total for recommended stack**: < $1.10/month (or < $0.10/month without full ZAP active scan).

---

## Recommendation

### Tier 1 — Deploy First (Week 1)

1. ~~**Cloudflare Tunnel**~~ — **DONE**. Already live at `lightlifeurbangardens.com`.
2. **Nuclei via Lambda** — deploy Nuclear Pond with Terraform. Feed it the Swagger spec (147 endpoints at `/api-docs`). Broadest vulnerability coverage for near-zero cost. Schedule weekly via EventBridge.
3. **Mozilla Observatory** — single Lambda function that calls the free API. Validates HTTP security headers. Schedule weekly.

### Tier 2 — Add When Ready (Week 2-3)

4. **ZAP Baseline via CodeBuild** — adds spider-based passive scanning that Nuclei doesn't do (Nuclei doesn't crawl). Use the API scan mode with our Swagger spec at `/api-docs`.
5. **testssl.sh** — validates TLS configuration. Quick Lambda function.

### Tier 3 — Optional

6. **ZAP Full Scan** — only if Tier 1-2 reveals issues that need deeper active testing. This sends attack traffic.
7. **Nikto** — largely redundant if running ZAP + Nuclei. Skip unless a specific gap is identified.
8. **nmap** — limited value through a Cloudflare Tunnel (only sees 443). Only useful with Tailscale.

### Architecture Pattern

```
EventBridge (weekly cron)
    |
    +---> Lambda: Nuclei scan ------------> S3 (findings JSON)
    +---> Lambda: Observatory API call ----> S3 (header report)
    +---> Lambda: testssl.sh --------------> S3 (TLS report)
    +---> CodeBuild: ZAP baseline ---------> S3 (ZAP report)
                                                |
                                           SNS topic
                                                |
                                           Email / Slack notification
```

All findings land in S3. An optional Athena table lets you query findings across scans. An SNS topic fires on new findings for alerting.

### What This Does NOT Cover
- **SAST** (static analysis of source code) — run locally, not externally. Semgrep or CodeQL.
- **Dependency scanning** — `npm audit` in CI. Not an external scan.
- **Infrastructure-as-code scanning** — Checkov/tfsec for Terraform. Run locally.
- **Runtime protection** — WAF, rate limiting. Different concern.

---

## Sources

- [AWS Fargate Pen Testing Architecture](https://aws.amazon.com/blogs/architecture/modernize-your-penetration-testing-architecture-on-aws-fargate/)
- [ZAP Docker Baseline Scan](https://www.zaproxy.org/docs/docker/baseline-scan/)
- [ZAP Docker Images (GHCR)](https://www.zaproxy.org/blog/2023-06-13-ghcr-docker-images/)
- [Nuclear Pond (Nuclei + Lambda)](https://github.com/DevSecOpsDocs/nuclearpond)
- [Nuclear Pond Terraform Module](https://github.com/DevSecOpsDocs/terraform-nuclear-pond)
- [Nuclei GitHub](https://github.com/projectdiscovery/nuclei)
- [Nikto GitHub](https://github.com/sullo/nikto)
- [testssl.sh](https://testssl.sh/)
- [Mozilla HTTP Observatory API](https://github.com/mozilla/http-observatory/blob/main/httpobs/docs/api.md)
- [SecurityHeaders.com API](https://securityheaders.com/api/)
- [Cloudflare Tunnel Docs](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/)
- [nmap in Lambda (Auth0)](https://auth0.com/blog/aws-increases-security-scan-freedom/)
- [AWS Lambda Pricing](https://aws.amazon.com/lambda/pricing/)
- [AWS Fargate Pricing](https://aws.amazon.com/fargate/pricing/)
- [AWS CodeBuild Pricing](https://aws.amazon.com/codebuild/pricing/)
- [AWS Inspector / GuardDuty / Security Hub comparison](https://www.intruder.io/blog/aws-security-services)
