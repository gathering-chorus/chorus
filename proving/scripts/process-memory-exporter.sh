#!/usr/bin/env bash
# @domain: metrics
# process-memory-exporter.sh — per-process & per-workload CPU/RSS plus daily load peak (#3714, #4992).
#
# Closes the three #4992 load-attribution blind spots measured 2026-11-17:
#   1. Sub-50MB high-CPU processes (clang++/cc1plus during oxigraph builds, rustc, cargo, mds)
#      no longer vanish below an RSS-only threshold: any group >= 1.0% CPU or >= 50MB RSS exports.
#   2. Multi-instance binaries (`node` across prod API, Clearing, Gathering, Jest, and orphaned
#      chorus-werk/<role>-<card> demo servers) disaggregate into `host_workload_*` by workload + owner.
#   3. Direct host load (`host_load_average{window="1m|5m|15m"}`) plus `host_load1_daily_max`
#      persisted per UTC date so intra-day spikes survive transient Prometheus scrape stalls.

set -euo pipefail
# launchd units (com.observability.process-memory, com.chorus.dagu) often omit /usr/sbin from PATH
export PATH="/usr/sbin:/sbin:/usr/bin:/bin:${PATH:-}"

CALLER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CALLER_ROOT="$(cd "$CALLER_DIR/.." && pwd)"
if [ -d "$CALLER_ROOT/data/textfile_collector" ] || [ "$(basename "$CALLER_ROOT")" = "shared-observability" ]; then
  DEFAULT_TF="$CALLER_ROOT/data/textfile_collector"
else
  DEFAULT_TF="$(cd "$CALLER_DIR/../../.." && pwd)/shared-observability/data/textfile_collector"
fi
TEXTFILE_DIR="${TEXTFILE_DIR:-$DEFAULT_TF}"
OUTFILE="${TEXTFILE_DIR}/process_memory.prom"
TMPFILE="${OUTFILE}.tmp.$$"
PS_TMP="${OUTFILE}.ps.$$"
MAX_FILE="${TEXTFILE_DIR}/.load1_daily_max"
PS_BIN="${PS_CMD:-/bin/ps}"
SYSCTL_BIN="${SYSCTL_CMD:-/usr/sbin/sysctl}"

cleanup() {
  rm -f "$TMPFILE" "$PS_TMP"
}
trap cleanup EXIT

mkdir -p "$TEXTFILE_DIR"

if [ -n "${PS_INPUT_FILE:-}" ]; then
  if [ ! -r "$PS_INPUT_FILE" ]; then
    echo "process-memory-exporter: cannot read PS_INPUT_FILE=$PS_INPUT_FILE" >&2
    exit 1
  fi
  cp "$PS_INPUT_FILE" "$PS_TMP"
else
  if ! "$PS_BIN" -Ao user=,rss=,pcpu=,command= > "$PS_TMP"; then
    echo "process-memory-exporter: ps failed ($PS_BIN)" >&2
    exit 1
  fi
fi

if [ -n "${LOADAVG_OVERRIDE:-}" ]; then
  load_raw="$LOADAVG_OVERRIDE"
else
  if ! load_raw="$("$SYSCTL_BIN" -n vm.loadavg | tr -d '{}')"; then
    echo "process-memory-exporter: sysctl failed ($SYSCTL_BIN)" >&2
    exit 1
  fi
fi

read -r l1 l5 l15 _ <<< "$load_raw"
if [ -z "${l1:-}" ] || [ -z "${l5:-}" ] || [ -z "${l15:-}" ]; then
  echo "process-memory-exporter: invalid loadavg '$load_raw'" >&2
  exit 1
fi

today="${TODAY_OVERRIDE:-$(date -u +%Y-%m-%d)}"
daily_max="$(awk -v td="$today" -v cur="$l1" -v mf="$MAX_FILE" 'BEGIN {
  pd = ""; pm = 0.0;
  if ((getline line < mf) > 0) {
    n = split(line, parts, " ");
    if (n >= 2) { pd = parts[1]; pm = parts[2] + 0.0; }
    close(mf);
  }
  m = (td == pd) ? pm : 0.0;
  c = cur + 0.0;
  if (c > m) m = c;
  printf "%.2f", m;
}')"
printf '%s %s\n' "$today" "$daily_max" > "${MAX_FILE}.tmp.$$"
mv "${MAX_FILE}.tmp.$$" "$MAX_FILE"

awk -v l1="$l1" -v l5="$l5" -v l15="$l15" -v lmax="$daily_max" '
function clean_label(s,    r) {
  r = s
  gsub(/[^a-zA-Z0-9_.:-]/, "_", r)
  return r
}
function basename_of(tok,    n, parts) {
  n = split(tok, parts, "/")
  return parts[n]
}
{
  user = $1
  rss_b = ($2 + 0) * 1024
  pcpu = $3 + 0.0
  # reconstruct command from field 4 onward
  cmd = ""
  for (i = 4; i <= NF; i++) {
    cmd = (i == 4) ? $i : (cmd " " $i)
  }
  if (cmd == "") next

  split(cmd, cparts, " ")
  bin = basename_of(cparts[1])

  # 1. Backwards-compatible process-name aggregation
  p_rss[bin] += rss_b
  p_cpu[bin] += pcpu
  p_cnt[bin] += 1

  # 2. Workload + owner classification
  wl = ""
  own = user
  if (match(cmd, /chorus-werk\/[a-zA-Z0-9_-]+\//)) {
    werk_seg = substr(cmd, RSTART + 12, RLENGTH - 13)
    if (cmd ~ /(server\.js|app\.js|directing\/clearing|platform\/api)/ && cmd !~ /jest/) {
      wl = "werk-demo-server"
      own = werk_seg
    }
  }
  if (wl == "") {
    if (bin ~ /^(clang|clang\+\+|cc1|cc1plus|c\+\+|ld)$/ || cmd ~ /\/(clang\+\+|cc1plus)(\s|$)/) {
      wl = "cxx-build"
    } else if (bin ~ /^(rustc|cargo|rustdoc|cargo-llvm-cov)$/) {
      wl = "rust-build"
    } else if (bin == "bats" || cmd ~ /\/bats(-exec-[a-z]+)?(\s|$)/) {
      wl = "bats-tests"
    } else if (cmd ~ /\/jest(\.js)?(\s|$)/ || cmd ~ /node_modules\/\.bin\/jest/) {
      wl = "jest-tests"
    } else if (bin == "promtail") {
      wl = "promtail"
    } else if (cmd ~ /org\.apache\.jena\.fuseki/ || cmd ~ /fuseki-server\.jar/) {
      wl = "fuseki"
    } else if (bin == "loki") {
      wl = "loki"
    } else if (bin == "prometheus") {
      wl = "prometheus"
    } else if (bin == "grafana" || bin == "grafana-server") {
      wl = "grafana"
    } else if (bin == "dagu") {
      wl = "dagu"
    } else if (bin == "vikunja") {
      wl = "vikunja"
    } else if (bin ~ /^chorus-(hooks|hook-shim|inject)$/) {
      wl = "chorus-hooks"
    } else if (bin ~ /^athena-(make|model|deploy|validate)$/) {
      wl = "athena-door"
    } else if (bin == "claude") {
      wl = "agent-claude"
    } else if (bin == "agy") {
      wl = "agent-agy"
    } else if (bin == "node") {
      if (cmd ~ /\/chorus\/platform\/api\//) wl = "chorus-api"
      else if (cmd ~ /\/chorus\/directing\/clearing\//) wl = "chorus-clearing"
      else if (cmd ~ /(jeff-bridwell-personal-site|dist\/app\.js)/) wl = "gathering-app"
      else if (cmd ~ /(ts-node|directing\/products\/cards)/) wl = "cards-cli"
      else wl = "node-other"
    } else if (bin ~ /^(mds|mds_stores|mdworker|mdworker_shared)$/) {
      wl = "macos-spotlight"
    } else if (bin ~ /^(mlhostd|ODDIPoirotExperimentationExtension|siriactionsd)$/) {
      wl = "macos-ml-siri"
    } else {
      wl = "other:" bin
    }
  }

  key = clean_label(wl) SUBSEP clean_label(own)
  w_rss[key] += rss_b
  w_cpu[key] += pcpu
  w_cnt[key] += 1
  w_is_named[key] = (substr(wl, 1, 6) != "other:") ? 1 : 0
}
END {
  split("java ollama llama-server claude agy node loki promtail prometheus grafana vikunja dagu rustc cargo clang clang++ cc1plus mds mds_stores mlhostd rsync ugrep chorus-hooks", want, " ")
  for (i in want) seen[want[i]] = 1
  for (p in p_rss) {
    if (p_rss[p] > 52428800 || p_cpu[p] >= 1.0 || (p in seen)) keep_p[p] = 1
  }
  for (p in seen) {
    if (!(p in p_rss)) {
      p_rss[p] = 0; p_cpu[p] = 0.0; p_cnt[p] = 0
      keep_p[p] = 1
    }
  }

  print "# HELP host_process_resident_memory_bytes Aggregated RSS by process name."
  print "# TYPE host_process_resident_memory_bytes gauge"
  for (p in keep_p) {
    printf "host_process_resident_memory_bytes{name=\"%s\"} %d\n", clean_label(p), p_rss[p]
  }
  print "# HELP host_process_cpu_percent Aggregated CPU percent by process name."
  print "# TYPE host_process_cpu_percent gauge"
  for (p in keep_p) {
    printf "host_process_cpu_percent{name=\"%s\"} %.1f\n", clean_label(p), p_cpu[p]
  }
  print "# HELP host_process_count Process count by name."
  print "# TYPE host_process_count gauge"
  for (p in keep_p) {
    printf "host_process_count{name=\"%s\"} %d\n", clean_label(p), p_cnt[p]
  }

  for (k in w_rss) {
    if (w_is_named[k] || w_rss[k] >= 52428800 || w_cpu[k] >= 1.0) keep_w[k] = 1
  }
  print "# HELP host_workload_resident_memory_bytes Aggregated RSS by disaggregated workload and owner."
  print "# TYPE host_workload_resident_memory_bytes gauge"
  for (k in keep_w) {
    split(k, kp, SUBSEP)
    printf "host_workload_resident_memory_bytes{workload=\"%s\",owner=\"%s\"} %d\n", kp[1], kp[2], w_rss[k]
  }
  print "# HELP host_workload_cpu_percent Aggregated CPU percent by disaggregated workload and owner."
  print "# TYPE host_workload_cpu_percent gauge"
  for (k in keep_w) {
    split(k, kp, SUBSEP)
    printf "host_workload_cpu_percent{workload=\"%s\",owner=\"%s\"} %.1f\n", kp[1], kp[2], w_cpu[k]
  }
  print "# HELP host_workload_process_count Process count by disaggregated workload and owner."
  print "# TYPE host_workload_process_count gauge"
  for (k in keep_w) {
    split(k, kp, SUBSEP)
    printf "host_workload_process_count{workload=\"%s\",owner=\"%s\"} %d\n", kp[1], kp[2], w_cnt[k]
  }

  print "# HELP host_load_average System load average over 1m, 5m, and 15m windows."
  print "# TYPE host_load_average gauge"
  printf "host_load_average{window=\"1m\"} %.2f\n", l1 + 0.0
  printf "host_load_average{window=\"5m\"} %.2f\n", l5 + 0.0
  printf "host_load_average{window=\"15m\"} %.2f\n", l15 + 0.0
  print "# HELP host_load1_daily_max Highest 1-minute load average observed by the exporter today (UTC)."
  print "# TYPE host_load1_daily_max gauge"
  printf "host_load1_daily_max %.2f\n", lmax + 0.0
}
' "$PS_TMP" > "$TMPFILE"

mv "$TMPFILE" "$OUTFILE"
