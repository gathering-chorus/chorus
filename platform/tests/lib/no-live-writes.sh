#!/bin/bash
# #4142 — fail if a jest source issues POST/PUT/DELETE against a live
# subdomain-entity, discover or reload route. Allowed: /api/athena/validate
# (a pure validator, no graph mutation). A fetch() call spans lines, so the
# method line is judged against the URL within the 4 lines above it.
f="$1"
awk '
  { buf[NR]=$0 }
  /method: '\''(POST|PUT|DELETE)'\''/ {
    url=""; for (k=NR; k>=NR-4 && k>0; k--) if (buf[k] ~ /\/api\/athena\//) { url=buf[k]; break }
    if (url ~ /\/api\/athena\/validate/) next
    if (url ~ /subdomains|discover-code|discover-tests|reload/) { print "live write at line " NR ": " url; bad=1 }
  }
  END { exit bad ? 1 : 0 }
' "$f"
