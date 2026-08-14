#!/usr/bin/env bash
# #3870 — service harvester, walk phase: observe launchd on this machine (and
# bedroom over ssh when reachable) into a snapshot JSON for the generator.
#
# Scope: ALL non-apple com.* units. NO allow-list — pair decision (wren,
# 2026-08-14): "an allow-list only finds what someone remembered to add" —
# proven the same hour by com.security.css (Identity's ONLY instance),
# com.prometheus.node-exporter, and com.ollama.ollama all sitting outside the
# old com.gathering|com.chorus filter. Externals (com.microsoft.*, com.openssh)
# appear too — the generator marks them external from binary-path evidence;
# unmapped stays the trend-to-zero number.
set -euo pipefail

OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    *) echo "service-harvest-walk: unknown arg $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { echo "service-harvest-walk: --out required" >&2; exit 2; }

python3 - "$OUT" <<'PYEOF'
import json, os, plistlib, re, subprocess, sys, datetime

out_path = sys.argv[1]
now = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

def keep(label):
    return label.startswith("com.") and not label.startswith("com.apple")

def classify(path):
    try:
        with open(path, 'rb') as f:
            p = plistlib.load(f)
    except Exception:
        return None
    kind = "job" if ("StartCalendarInterval" in p or "StartInterval" in p) else "service"
    argv = p.get("ProgramArguments") or ([p["Program"]] if p.get("Program") else [])
    # FULL argv, not argv0 — most of our units launch as /bin/bash <our-script>;
    # argv0-only marked ~70 of our own units external on the first live run
    # (2026-08-14, third instance of the same evidence bug in one hour).
    return {"kind": kind,
            "keepAlive": bool(p.get("KeepAlive")),
            "interval": p.get("StartInterval"),
            "calendar": bool(p.get("StartCalendarInterval")),
            "argv": argv,
            "program": argv[0] if argv else ""}

PLIST_DIRS = [os.path.expanduser("~/Library/LaunchAgents"),
              "/Library/LaunchAgents", "/Library/LaunchDaemons"]

def find_plist(label):
    for d in PLIST_DIRS:
        pp = os.path.join(d, label + ".plist")
        if os.path.exists(pp):
            return pp
    return None

def argv_from_print(label, uid):
    # SMAppService/app-bundle units (docker, ollama, VSCode.ShipIt) have no
    # on-disk plist in the three dirs; `launchctl print` still exposes the
    # arguments array and bundle identity. Evidence, not guesswork.
    # 2026-08-14: 9 of 19 live "unmapped" were externals invisible to the
    # plist-only walk — fixtures green, live wrong, the recurring shape.
    try:
        out = subprocess.run(["launchctl", "print", f"gui/{uid}/{label}"],
                             capture_output=True, text=True, timeout=5).stdout
    except Exception:
        return [], None
    argv, in_args = [], False
    bundle = None
    for line in out.splitlines():
        t = line.strip()
        if t.startswith("arguments = {"):
            in_args = True; continue
        if in_args:
            if t == "}":
                in_args = False
            elif t:
                argv.append(t)
        m = re.match(r"parent bundle identifier = (\S+)", t)
        if m:
            bundle = m.group(1)
    return argv, bundle

def walk_local():
    units = {}
    uid = os.getuid()
    lc = subprocess.run(["launchctl", "list"], capture_output=True, text=True).stdout
    for line in lc.splitlines():
        parts = line.split("\t")
        if len(parts) != 3 or not keep(parts[2]):
            continue
        pid, exit_code, label = parts
        rec = {"pid": pid if pid != "-" else None,
               "runState": "running" if pid != "-" else "loaded",
               "lastExitCode": int(exit_code) if exit_code.lstrip('-').isdigit() else None}
        pp = find_plist(label)
        if pp:
            meta = classify(pp)
            if meta:
                rec.update(meta)
        if not rec.get("program"):
            argv, bundle = argv_from_print(label, uid)
            if argv:
                rec["argv"] = argv
                rec["program"] = argv[0]
            if bundle:
                rec["bundleId"] = bundle
        units[label] = rec
    return units

def walk_bedroom():
    try:
        cmd = ("launchctl list | awk -F'\t' '$3 ~ /^com\\./ && $3 !~ /^com\\.apple/' ; "
               "echo ---PLISTS--- ; "
               # three plist dirs — bedroom leaked ollama/openssh/docker (wren
               # 16:14). NO launchctl-print over ssh: the gui domain is not
               # reliably reachable from a non-interactive session, and the
               # first attempt broke plist parsing entirely (walk-live5).
               # bedroom's login shell is zsh: an unmatched glob is FATAL — even inside
               # $(ls ...) zsh expands first (two failed rounds prove it). find does
               # its own matching, no shell glob anywhere.
               "for f in $(find ~/Library/LaunchAgents /Library/LaunchAgents /Library/LaunchDaemons -maxdepth 1 -name 'com.*.plist' 2>/dev/null); do "
               "case \"$f\" in *com.apple*) continue;; esac; "
               "[ -f \"$f\" ] && echo \"FILE:$f\" && plutil -convert json -o - \"$f\" 2>/dev/null && echo; done")
        remote = subprocess.run(["ssh", "-o", "ConnectTimeout=5", "bedroom", cmd],
                                capture_output=True, text=True, timeout=30).stdout
    except Exception as e:
        return {"error": str(e)}
    units = {}
    head, _, plists = remote.partition("---PLISTS---")
    for line in head.splitlines():
        parts = line.split("\t")
        if len(parts) != 3:
            continue
        pid, exit_code, label = parts
        units[label] = {"pid": pid if pid != "-" else None,
                        "runState": "running" if pid != "-" else "loaded",
                        "lastExitCode": int(exit_code) if exit_code.lstrip('-').isdigit() else None}
    cur, buf = None, []
    def absorb(cur, buf):
        if not cur or not buf:
            return
        try:
            p = json.loads("\n".join(buf))
            label = os.path.basename(cur)[:-6]
            if label in units:
                kind = "job" if ("StartCalendarInterval" in p or "StartInterval" in p) else "service"
                argv = p.get("ProgramArguments") or ([p["Program"]] if p.get("Program") else [])
                units[label].update({"kind": kind, "keepAlive": bool(p.get("KeepAlive")),
                                     "interval": p.get("StartInterval"),
                                     "calendar": bool(p.get("StartCalendarInterval")),
                                     "argv": argv,
                                     "program": argv[0] if argv else ""})
        except Exception:
            pass
    for line in plists.splitlines():
        if line.startswith("FILE:"):
            absorb(cur, buf)
            cur, buf = line[5:], []
        else:
            buf.append(line)
    absorb(cur, buf)
    # Third evidence state (wren 16:20): a remote unit whose plist we could
    # not find gets evidenceUnavailable — "we could not look" is an ops gap,
    # not "nobody claims it". Cleared when the per-machine agent runs locally.
    for rec in units.values():
        if "kind" not in rec and not rec.get("argv") and not rec.get("bundleId"):
            rec["evidenceUnavailable"] = True
    return units

out = {"harvestedAt": now,
       "machines": {"library": walk_local(), "bedroom": walk_bedroom()}}
json.dump(out, open(out_path, "w"), indent=1, sort_keys=True)
lib = out["machines"]["library"]
bed = out["machines"]["bedroom"]
bed_n = len(bed) if "error" not in bed else f"error:{bed['error'][:40]}"
print(f"walk: library {len(lib)} units, bedroom {bed_n}", file=sys.stderr)
PYEOF
