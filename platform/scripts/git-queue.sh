#!/bin/bash
# MIGRATE: TypeScript P3 — DEC-100 (no bash APIs)
# git-queue.sh — FIFO commit lock for multi-role team repo
#
# Three roles share one git repo. Concurrent git add + commit can corrupt
# the index. This script serializes commits using kernel-level advisory
# locking via macOS lockf(1).
#
# Usage:
#   git-queue.sh commit <files...> -- -m "message"
#   git-queue.sh status
#   git-queue.sh help
#
# Role identification: set DEPLOY_ROLE env var (e.g., DEPLOY_ROLE=silas)

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "${CHORUS_ROOT}")"
LOCK_FILE="${REPO_ROOT}/.git-commit.lock"
META_FILE="/tmp/chorus-git-commit.meta"
CHORUS_LOG="${REPO_ROOT}/platform/scripts/chorus-log"
LOCK_TIMEOUT=30
ROLE="${DEPLOY_ROLE:-unknown}"

# --- Helpers ---

log_event() {
  local event="$1"; shift
  "$CHORUS_LOG" "$event" "$ROLE" "$@" >/dev/null 2>/dev/null || true
}

write_meta() {
  printf '%s|%s|%s\n' "$ROLE" "$$" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$META_FILE"
}

clear_meta() {
  rm -f "$META_FILE"
}

show_status() {
  if [ -f "$META_FILE" ]; then
    local meta
    meta=$(cat "$META_FILE")
    local holder="${meta%%|*}"
    local rest="${meta#*|}"
    local pid="${rest%%|*}"
    local ts="${rest#*|}"
    # Check if holding process is still alive
    if kill -0 "$pid" 2>/dev/null; then
      echo "git-queue: locked by ${holder} (PID ${pid}) since ${ts}"
    else
      echo "git-queue: stale lock (${holder} PID ${pid} dead) — clearing"
      clear_meta
      rm -f "$LOCK_FILE"
    fi
  else
    echo "git-queue: free"
  fi
}

usage() {
  cat <<'EOF'
git-queue.sh — FIFO commit lock for multi-role team repo

Commands:
  commit <files...> -- -m "message"   Acquire lock, stage, commit, release
  status                              Show lock holder or "free"
  help                                This message

Environment:
  DEPLOY_ROLE   Role name for attribution (silas, wren, kade)

Examples:
  DEPLOY_ROLE=silas git-queue.sh commit architect/ -- -m "silas: ADR-016"
  DEPLOY_ROLE=wren git-queue.sh commit roles/wren/ -- -m "wren: briefs"
  git-queue.sh status
EOF
}

# --- Commands ---

ensure_hooks_installed() {
  # #2465: auto-install tracked hooks if the installed pre-commit isn't a symlink
  # to the canonical source. Safe to run every commit — install script is idempotent.
  local canonical="$REPO_ROOT/platform/hooks/pre-commit"
  local installed="$REPO_ROOT/.git/hooks/pre-commit"
  [ -f "$canonical" ] || return 0
  if [ ! -L "$installed" ] || [ "$(readlink "$installed")" != "$canonical" ]; then
    bash "$REPO_ROOT/platform/scripts/install-hooks.sh" >/dev/null 2>&1 || true
  fi
}

do_commit() {
  ensure_hooks_installed
  # Split args at -- into files and git-commit flags
  local files=()
  local git_args=()
  local past_separator=false

  for arg in "$@"; do
    if [ "$arg" = "--" ]; then
      past_separator=true
      continue
    fi
    if $past_separator; then
      git_args+=("$arg")
    else
      files+=("$arg")
    fi
  done

  if [ ${#files[@]} -eq 0 ]; then
    echo "git-queue: error — no files specified" >&2
    echo "Usage: git-queue.sh commit <files...> -- -m \"message\"" >&2
    exit 1
  fi

  if [ ${#git_args[@]} -eq 0 ]; then
    echo "git-queue: error — no commit message (use -- -m \"message\")" >&2
    exit 1
  fi

  # Extract commit message for spine context
  local _msg=""
  for _i in "${!git_args[@]}"; do
    if [ "${git_args[$_i]}" = "-m" ] && [ $((_i + 1)) -lt ${#git_args[@]} ]; then
      _msg="${git_args[$((_i + 1))]}"
      _msg="${_msg%%$'\n'*}"  # first line only
      break
    fi
  done

  log_event "build.queue.waiting" "file_count=${#files[@]}" "message=${_msg:0:80}"

  # Acquire lock with timeout — lockf is kernel-level, auto-releases on process death
  # Exit 75 = timeout (EX_TEMPFAIL)
  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    log_event "build.queue.timedout" "timeout=${LOCK_TIMEOUT}s"
    echo "git-queue: timeout after ${LOCK_TIMEOUT}s — another commit is holding the lock" >&2
    show_status
    exit 75
  fi

  # Lock acquired
  write_meta
  log_event "build.queue.acquired" "file_count=${#files[@]}"

  # Export marker so pre-commit hook knows we're in the queue
  export _GIT_QUEUE_INTERNAL=1
  # Export commit message for pre-commit WIP gate swat/chore bypass (#1799)
  export _COMMIT_MSG="${_msg}"

  # Doc-drift check (#763) — warn if related docs may be stale
  local drift_conf
  drift_conf="$(dirname "$0")/doc-drift.conf"
  if [ -f "$drift_conf" ]; then
    local drift_warnings=()
    while IFS= read -r line; do
      [[ "$line" =~ ^#.*$ || -z "$line" ]] && continue
      local code_glob doc_path
      code_glob=$(echo "$line" | sed 's/ *→.*//' | xargs)
      doc_path=$(echo "$line" | sed 's/.*→ *//' | xargs)
      [ -z "$code_glob" ] || [ -z "$doc_path" ] && continue
      local code_match=false
      for f in "${files[@]}"; do
        case "$f" in
          $code_glob*) code_match=true; break ;;
        esac
      done
      if $code_match; then
        local doc_found=false
        for f in "${files[@]}"; do
          case "$f" in
            $doc_path*) doc_found=true; break ;;
          esac
        done
        if ! $doc_found; then
          drift_warnings+=("$doc_path")
        fi
      fi
    done < "$drift_conf"
    if [ ${#drift_warnings[@]} -gt 0 ]; then
      echo "git-queue: doc-drift BLOCKED — update these docs before committing:" >&2
      printf '  %s\n' "${drift_warnings[@]}" >&2
      echo "git-queue: add the doc(s) to your file list, or pass DOC_DRIFT_SKIP=1 to override" >&2
      if [ "${DOC_DRIFT_SKIP:-0}" != "1" ]; then
        clear_meta
        log_event "build.queue.blocked" "reason=doc-drift" "count=${#drift_warnings[@]}"
        return 1
      fi
    fi
  fi

  # Ontology validation (#1356) — if chorus.ttl is being committed, validate against version contract
  local ttl_changed=false
  for f in "${files[@]}"; do
    case "$f" in
      *chorus.ttl*) ttl_changed=true; break ;;
    esac
  done
  if $ttl_changed; then
    local validate_script="$(dirname "$0")/ontology-validate.sh"
    if [ -x "$validate_script" ]; then
      local validate_output
      validate_output=$("$validate_script" 9>&- 2>&1)
      local validate_exit=$?
      if [ $validate_exit -ne 0 ]; then
        echo "$validate_output"
        echo "git-queue: ontology validation FAILED — fix violations before committing chorus.ttl" >&2
        clear_meta
        log_event "build.queue.blocked" "reason=ontology-validation"
        return 1
      fi
      echo "$validate_output" | grep -v '__VERSION__'
      log_event "build.ontology.validated" "status=pass"

      # Spine event on version change (#1356 AC7)
      local new_version
      new_version=$(echo "$validate_output" | grep '__VERSION__' | sed 's/__VERSION__://')
      local version_file="/tmp/chorus-ontology-version"
      local old_version
      old_version=$(cat "$version_file" 2>/dev/null || echo "")
      if [ -n "$new_version" ] && [ "$new_version" != "$old_version" ]; then
        echo "$new_version" > "$version_file"
        if [ -n "$old_version" ]; then
          log_event "ontology.version.changed" "old=$old_version" "new=$new_version"
          echo "git-queue: ontology version changed $old_version → $new_version"
        else
          echo "$new_version" > "$version_file"
        fi
      fi
    fi
  fi

  # Stage and commit — pass files to BOTH git add and git commit.
  # Without files on git commit, any previously staged files from another
  # role would leak into this commit (cross-role staging collision).
  # Close fd 9 for child processes so git credential-cache-daemon
  # doesn't inherit the lockf descriptor and hold the lock forever.
  local exit_code=0
  git add "${files[@]}" 9>&- && git commit "${git_args[@]}" -- "${files[@]}" 9>&- || exit_code=$?

  # #2193: emit commit.landed after successful commit. Semantic event for
  # the coherence checker to correlate role activity with real git history.
  if [ $exit_code -eq 0 ]; then
    local _sha
    _sha=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
    local _subject
    _subject=$(printf '%s' "$_msg" | head -1)
    local _card_id=""
    if [[ "$_msg" =~ \#([0-9]+) ]]; then
      _card_id="${BASH_REMATCH[1]}"
    fi
    log_event "commit.landed" \
      "sha=${_sha}" \
      "author_role=${ROLE}" \
      "card_id=${_card_id}" \
      "files_changed=${#files[@]}" \
      "message_subject=${_subject:0:100}"
  fi

  # Release
  clear_meta
  # fd 9 closes on script exit, releasing lockf
  log_event "build.queue.released" "exit_code=${exit_code}" "file_count=${#files[@]}" "message=${_msg:0:80}"

  if [ $exit_code -ne 0 ]; then
    echo "git-queue: commit failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Push with dirty-tree handling (#1780) ---
# Prior work: two stash-drop incidents lost work when roles manually stashed.
# This is NOT manual stash — it's atomic stash-rebase-pop inside the lock.
# The lock prevents concurrent push/commit. Stash always pops immediately.

do_push() {
  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: push timeout — lock held" >&2
    exit 75
  fi
  write_meta
  log_event "build.push.started"

  local dirty
  dirty=$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null | grep -v '^?' | head -1)

  local stashed=false
  if [ -n "$dirty" ]; then
    git -C "$REPO_ROOT" stash --quiet 9>&- 2>/dev/null
    stashed=true
  fi

  local exit_code=0
  git -C "$REPO_ROOT" pull --rebase 9>&- 2>&1 && git -C "$REPO_ROOT" push 9>&- 2>&1 || exit_code=$?

  if $stashed; then
    git -C "$REPO_ROOT" stash pop --quiet 9>&- 2>/dev/null || {
      local stash_ref
      stash_ref=$(git -C "$REPO_ROOT" stash list | head -1)
      echo "git-queue: WARNING — stash pop failed (conflict). Your files are safe in: ${stash_ref:-stash@{0}}" >&2
      echo "git-queue: recover with: git stash pop (resolve conflicts) or git stash show" >&2
      log_event "build.push.stash_pop_failed" "stash_ref=${stash_ref:-unknown}"
    }
  fi

  clear_meta
  log_event "build.push.completed" "exit_code=${exit_code}"

  if [ $exit_code -ne 0 ]; then
    echo "git-queue: push failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Main ---

cmd="${1:-help}"
shift || true

case "$cmd" in
  commit)  do_commit "$@" ;;
  push)    do_push ;;
  add)
    echo "git-queue: 'add' is not a command — did you mean 'commit'?" >&2
    echo "  git-queue.sh commit <files...> -- -m \"message\"" >&2
    echo "  (stages AND commits in one atomic operation)" >&2
    exit 1
    ;;
  status)  show_status ;;
  help|--help|-h)  usage ;;
  silas|wren|kade)
    echo "git-queue: old syntax detected — use this instead:" >&2
    echo "  cd /Users/jeffbridwell/CascadeProjects && DEPLOY_ROLE=${cmd} bash messages/scripts/git-queue.sh commit <your-dir>/ -- -m \"${cmd}: ${1:-your message}\"" >&2
    exit 1
    ;;
  *)
    echo "git-queue: unknown command '${cmd}'" >&2
    usage
    exit 1
    ;;
esac
