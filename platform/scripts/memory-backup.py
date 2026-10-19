#!/usr/bin/env python3
"""memory-backup.py — #4630. Dated daily copies of the team's memory: messages
and streams. Jeff, 2026-10-18: "our messages and streams are arguably our most
valuable long term data set besides our repo"; he fears "memory loss".

The nightly rsync is a mirror (--delete): it keeps no history and copies damage
over the good copy. This keeps one folder per Boston day that never changes once
written, in three places (Library, Bedroom, Google Drive), each file with its
sha256, and checks every copy by hash. Anything short is red, never "OK".

  memory-backup.py [YYYY-MM-DD]     the day to keep (default: yesterday, Boston)

What a day holds:
  spine-<day>.jsonl.gz      that day's spine lines, cut by the event's own time
                            (a line with no readable time goes with the line
                            before it), so restoring a day is one file
  messages-<day>.db.gz      a consistent snapshot of pulse's messages.db
  buzz-<day>.sql.gz         pg_dump of the Buzz relay's Postgres on Bedroom
                            (room replies live only there)
  buzz-minio-<day>.tar.gz   the relay's attachments (Minio)
  transcripts-<day>.tar.gz  every transcript line of that day, by its own time
  nightly-suites-<day>.log.gz  the nightly suite log (the /nightly page's source)
  SHA256SUMS                one line per file

Env (all have defaults): MEMORY_SPINE, MEMORY_MESSAGES_DB, MEMORY_BUZZ_PG_CMD,
MEMORY_BUZZ_MINIO_CMD (shell commands that write the dump to stdout),
MEMORY_TRANSCRIPTS, MEMORY_LOCAL, MEMORY_DRIVE, MEMORY_REMOTE (host:dir, or a
plain dir for tests), MEMORY_KEEP_DAYS (default 45).
Exit 0 green · 1 red (a place is missing or a copy's hash differs) · 2 usage.
"""
import datetime
import gzip
import hashlib
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tarfile
import zoneinfo

HOME = os.path.expanduser("~")
BOSTON = zoneinfo.ZoneInfo("America/New_York")
SPINE = os.environ.get("MEMORY_SPINE", f"{HOME}/.chorus/chorus.log")
MESSAGES_DB = os.environ.get("MEMORY_MESSAGES_DB", f"{HOME}/CascadeProjects/chorus/platform/pulse/messages.db")
BEDROOM = "Jeffs-Mac-mini.local"
DOCKER = "export PATH=$PATH:/usr/local/bin:/opt/homebrew/bin; docker exec"
BUZZ_PG_CMD = os.environ.get("MEMORY_BUZZ_PG_CMD", f"ssh -o ConnectTimeout=10 {BEDROOM} '{DOCKER} buzz-prod-postgres-1 pg_dump -U buzz buzz'")
# the minio image has no tar; docker cp streams the folder as a tar
BUZZ_MINIO_CMD = os.environ.get("MEMORY_BUZZ_MINIO_CMD", f"ssh -o ConnectTimeout=10 {BEDROOM} 'export PATH=$PATH:/usr/local/bin:/opt/homebrew/bin; docker cp buzz-prod-minio-1:/data -'")
TRANSCRIPTS = os.environ.get("MEMORY_TRANSCRIPTS", f"{HOME}/.claude/projects")
NIGHTLY_LOG = os.environ.get("MEMORY_NIGHTLY_LOG", f"{HOME}/Library/Logs/Chorus/nightly-suites.log")
LOCAL = os.environ.get("MEMORY_LOCAL", f"{HOME}/Backups/chorus-memory")
DRIVE = os.environ.get("MEMORY_DRIVE", "")
REMOTE = os.environ.get("MEMORY_REMOTE", "Jeffs-Mac-mini.local:/Users/jeffbridwell/Backups/chorus-memory")
KEEP_DAYS = int(os.environ.get("MEMORY_KEEP_DAYS", "45"))

TS = re.compile(rb'"timestamp":"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.\d+)?(Z|[+-]\d\d:?\d\d)?"')


def drive_dir() -> str:
    """Google Drive's My Drive/chorus-backups; '' when Drive isn't signed in."""
    if DRIVE:
        return DRIVE
    cloud = f"{HOME}/Library/CloudStorage"
    for d in sorted(os.listdir(cloud)) if os.path.isdir(cloud) else []:
        if d.startswith("GoogleDrive-") and os.path.isdir(f"{cloud}/{d}/My Drive"):
            return f"{cloud}/{d}/My Drive/chorus-backups"
    return ""


def line_day(line: bytes) -> str | None:
    """The Boston day of a spine line, from its own timestamp; None when unreadable."""
    m = TS.search(line)
    if not m:
        return None
    off = (m.group(2) or b"Z").decode().replace("Z", "+00:00")
    if len(off) == 5:
        off = off[:3] + ":" + off[3:]
    try:
        t = datetime.datetime.fromisoformat(m.group(1).decode() + off)
    except ValueError:
        return None
    return t.astimezone(BOSTON).date().isoformat()


def seek_day(f, size: int, day: str) -> int:
    """A byte offset at or before the first line of `day` (the spine is in time order)."""
    lo, hi = 0, size
    while hi - lo > (1 << 20):
        mid = (lo + hi) // 2
        f.seek(mid)
        f.readline()
        d = None
        for _ in range(200):
            line = f.readline()
            if not line:
                break
            d = line_day(line)
            if d:
                break
        if d is None or d >= day:
            hi = mid
        else:
            lo = mid
    return lo


def spine_slice(day: str, out: str) -> int:
    """Write that day's spine lines, gzipped. Returns the line count."""
    n = 0
    size = os.path.getsize(SPINE)
    with open(SPINE, "rb") as f, gzip.open(out, "wb", compresslevel=6) as g:
        f.seek(seek_day(f, size, day))
        f.readline() if f.tell() else None
        current = None
        for line in f:
            d = line_day(line) or current
            current = d
            if d is None or d < day:
                continue
            if d > day:
                break
            g.write(line)
            n += 1
    return n


def sqlite_snapshot(src: str, out: str) -> None:
    """A consistent copy (sqlite's own backup), gzipped."""
    tmp = out + ".tmp"
    with sqlite3.connect(f"file:{src}?mode=ro", uri=True) as s, sqlite3.connect(tmp) as d:
        s.backup(d)
    with open(tmp, "rb") as r, gzip.open(out, "wb", compresslevel=6) as g:
        shutil.copyfileobj(r, g)
    os.remove(tmp)


def dump_to_gz(cmd: str, out: str, what: str) -> list:
    """Run a dump command (stdout), gzip it; a failed or empty dump is red, never kept."""
    r = subprocess.run(cmd, shell=True, capture_output=True, timeout=1800)
    if r.returncode or not r.stdout:
        return [f"{what}: dump failed (exit {r.returncode}): {r.stderr.decode(errors='replace').strip()[:160]}"]
    with gzip.open(out, "wb", compresslevel=6) as g:
        g.write(r.stdout)
    return []


def transcripts_of(day: str, out: str) -> int:
    """Every transcript line of `day`, by the line's own timestamp (a line with
    none goes with the line before), one tar member per session file. Picking
    whole files by mtime missed long sessions written on later days (Wren)."""
    import io
    start = datetime.datetime.fromisoformat(day + "T00:00:00").replace(tzinfo=BOSTON).timestamp()
    n = 0
    with tarfile.open(out, "w:gz") as t:
        for root, _, files in os.walk(TRANSCRIPTS):
            for name in sorted(files):
                if not name.endswith(".jsonl"):
                    continue
                p = os.path.join(root, name)
                if os.path.getmtime(p) < start:
                    continue  # last written before the day began: holds none of it
                buf = io.BytesIO()
                current = None
                with open(p, "rb") as f:
                    for line in f:
                        d = line_day(line) or current
                        current = d
                        if d == day:
                            buf.write(line)
                if buf.tell():
                    info = tarfile.TarInfo(os.path.relpath(p, TRANSCRIPTS))
                    info.size = buf.tell()
                    info.mtime = int(start)
                    buf.seek(0)
                    t.addfile(info, buf)
                    n += 1
    return n


def file_snapshot(src: str, out: str) -> list:
    """A whole-file copy, gzipped (the nightly suite log: the /nightly page's source)."""
    if not os.path.exists(src):
        return [f"{os.path.basename(src)}: not found at {src}"]
    with open(src, "rb") as r, gzip.open(out, "wb", compresslevel=6) as g:
        shutil.copyfileobj(r, g)
    return []


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sums_of(folder: str) -> dict:
    out = {}
    with open(os.path.join(folder, "SHA256SUMS")) as f:
        for line in f:
            digest, name = line.split(None, 1)
            out[name.strip()] = digest
    return out


def verify(folder: str, want: dict) -> list:
    """Every file in `want` is present in `folder` with the same sha256."""
    bad = []
    for name, digest in want.items():
        p = os.path.join(folder, name)
        if not os.path.exists(p):
            bad.append(f"{name} missing")
        elif sha256(p) != digest:
            bad.append(f"{name} sha256 differs")
    return bad


def remote_cmd(args: list) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True, text=True, timeout=1800)


def main(argv: list) -> int:
    if len(argv) > 2 or (len(argv) == 2 and not re.fullmatch(r"\d{4}-\d\d-\d\d", argv[1])):
        print(__doc__.split("\n\n")[2])
        return 2
    day = argv[1] if len(argv) == 2 else (datetime.datetime.now(BOSTON).date() - datetime.timedelta(days=1)).isoformat()
    red = []
    folder = os.path.join(LOCAL, day)
    os.makedirs(folder, exist_ok=True)
    # a day that is already kept is never rewritten: it is the record
    if not os.path.exists(os.path.join(folder, "SHA256SUMS")):
        lines = spine_slice(day, os.path.join(folder, f"spine-{day}.jsonl.gz"))
        print(f"memory-backup {day}: spine {lines} lines")
        if lines == 0:
            red.append("spine: 0 lines for the day")
        sqlite_snapshot(MESSAGES_DB, os.path.join(folder, f"messages-{day}.db.gz"))
        red += dump_to_gz(BUZZ_PG_CMD, os.path.join(folder, f"buzz-{day}.sql.gz"), "buzz postgres")
        red += dump_to_gz(BUZZ_MINIO_CMD, os.path.join(folder, f"buzz-minio-{day}.tar.gz"), "buzz minio")
        print(f"memory-backup {day}: transcripts {transcripts_of(day, os.path.join(folder, f'transcripts-{day}.tar.gz'))}")
        red += file_snapshot(NIGHTLY_LOG, os.path.join(folder, f"nightly-suites-{day}.log.gz"))
        if red:
            # a day missing a piece is not written as kept: the next run tries again
            for r in red:
                print(f"memory-backup {day}: RED {r}")
            print(f"memory-backup {day}: RED — the day is not kept, rerun after the cause is fixed")
            return 1
        with open(os.path.join(folder, "SHA256SUMS"), "w") as f:
            for name in sorted(os.listdir(folder)):
                if name != "SHA256SUMS" and not name.endswith(".tmp"):
                    f.write(f"{sha256(os.path.join(folder, name))}  {name}\n")
    want = sums_of(folder)
    red += [f"library: {b}" for b in verify(folder, want)]

    drive = drive_dir()
    if not drive or not os.path.isdir(os.path.dirname(drive)):
        red.append("drive: Google Drive is not signed in on Library (no My Drive folder)")
    else:
        shutil.copytree(folder, os.path.join(drive, day), dirs_exist_ok=True)
        red += [f"drive: {b}" for b in verify(os.path.join(drive, day), want)]

    if ":" in REMOTE:
        host, rdir = REMOTE.split(":", 1)
        r = remote_cmd(["nice", "-n", "10", "rsync", "-a", "--timeout=300", folder, f"{host}:{rdir}/"])
        if r.returncode:
            red.append(f"bedroom: rsync exit {r.returncode}")
        else:
            r = remote_cmd(["ssh", "-o", "ConnectTimeout=10", host, f"cd '{rdir}/{day}' && shasum -a 256 *"])
            got = {ln.split(None, 1)[1].strip(): ln.split(None, 1)[0] for ln in r.stdout.splitlines() if ln.strip()}
            for name, digest in want.items():
                if got.get(name) != digest:
                    red.append(f"bedroom: {name} {'missing' if name not in got else 'sha256 differs'}")
    else:
        shutil.copytree(folder, os.path.join(REMOTE, day), dirs_exist_ok=True)
        red += [f"bedroom: {b}" for b in verify(os.path.join(REMOTE, day), want)]

    cutoff = (datetime.date.fromisoformat(day) - datetime.timedelta(days=KEEP_DAYS)).isoformat()
    for old in sorted(os.listdir(LOCAL)):
        if re.fullmatch(r"\d{4}-\d\d-\d\d", old) and old < cutoff:
            shutil.rmtree(os.path.join(LOCAL, old))

    for r in red:
        print(f"memory-backup {day}: RED {r}")
    print(f"memory-backup {day}: {'RED' if red else 'OK'} — {len(want)} files, 3 places")
    return 1 if red else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
