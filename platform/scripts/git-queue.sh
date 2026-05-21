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

# #2571 — source-from-substrate replaces #1917's ${CHORUS_ROOT:-mac-path} default
source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "${CHORUS_ROOT}")"
LOCK_FILE="${REPO_ROOT}/.git-commit.lock"
META_FILE="/tmp/chorus-git-commit.meta"
CHORUS_LOG="${REPO_ROOT}/platform/scripts/chorus-log"
LOCK_TIMEOUT=30
ROLE="${DEPLOY_ROLE:-unknown}"

# --- Helpers ---

# #2580: refuse cross-role branch contamination at the queue layer.
# Defense-in-depth for the per-role-worktree convention (#2582). Returns 0 if
# branch matches the role's expected prefix (or escape hatch is set), 1 with
# a clear error message + spine event if mismatch.
check_branch() {
  local op="${1:-commit}"
  local force_flag="${2:-}"
  if [ "$force_flag" = "--force-branch" ]; then
    return 0
  fi

  if [ -z "${DEPLOY_ROLE:-}" ] || [ "$ROLE" = "unknown" ]; then
    echo "git-queue: error — DEPLOY_ROLE env var must be set (silas|wren|kade)" >&2
    echo "  example: DEPLOY_ROLE=kade bash platform/scripts/git-queue.sh ${op} <files>" >&2
    return 1
  fi

  local actual_branch
  actual_branch=$(git -C "$REPO_ROOT" symbolic-ref --short HEAD 2>/dev/null || echo "")
  if [ -z "$actual_branch" ]; then
    echo "git-queue: error — HEAD is detached (not on a branch)" >&2
    return 1
  fi

  # #2639: branch-prefix match delegated to branch-check.sh (single source
  # for the invariant; pre-push sources the same file).
  # shellcheck source=branch-check.sh
  source "$REPO_ROOT/platform/scripts/branch-check.sh"
  if branch_check_match "$ROLE" "$actual_branch"; then
    # #2641 mode-C: stricter inner check — branch must match the role's
    # currently-declared active-card-id from role-state. Catches the
    # same-role wrong-card case (e.g., kade declared building #2641 but
    # HEAD is on kade/2640-narrow).
    local _role_state_file="/tmp/claude-team-scan/${ROLE}-declared.json"
    local _active_card=""
    if [ -f "$_role_state_file" ]; then
      _active_card=$(python3 -c "
import json, re, sys
try:
    d = json.load(open('$_role_state_file'))
    detail = d.get('detail', '') or ''
    m = re.search(r'card=(\d+)', detail)
    print(m.group(1) if m else '')
except Exception:
    print('')
" 2>/dev/null || echo "")
    fi
    # Only enforce if role-state declares an active card. If empty, fall
    # through (we've already passed the outer prefix check).
    if [ -n "$_active_card" ]; then
      if branch_check_card_match "$ROLE" "$actual_branch" "$_active_card"; then
        return 0
      fi
      # Mismatch: same role, wrong card.
      local _cwd
      _cwd=$(pwd)
      log_event "commits.branch.card_mismatch_detected" \
        "expected=${ROLE}/${_active_card}" \
        "actual=${actual_branch}" \
        "op=${op}" \
        "cwd=${_cwd}" \
        "active_card_id=${_active_card}"
      echo "git-queue: ERROR — same-role wrong-card guard (#2641, mode C)" >&2
      echo "  role:            ${ROLE}" >&2
      echo "  active card:     ${ROLE}/${_active_card}" >&2
      echo "  actual branch:   ${actual_branch}" >&2
      echo "  cwd:             ${_cwd}" >&2
      echo "" >&2
      echo "  Self-recovery:" >&2
      echo "    git checkout ${ROLE}/${_active_card}-<slug>     # switch back" >&2
      echo "    role-state ${ROLE} building card=<actual>       # update declared card" >&2
      echo "" >&2
      echo "  Emergency override:" >&2
      echo "    bash $0 ${op} --force-branch <args>" >&2
      return 1
    fi
    return 0
  else
      # Forensic payload per Silas's review on #2580: cwd + commits_ahead
      # let dashboards distinguish "session-long contamination" from "one-typo".
      local _cwd _commits_ahead
      _cwd=$(pwd)
      _commits_ahead=$(git -C "$REPO_ROOT" rev-list --count "@{upstream}..HEAD" 2>/dev/null || echo "0")
      log_event "commits.branch.mismatch_detected" \
        "expected=${ROLE}/*" \
        "actual=${actual_branch}" \
        "op=${op}" \
        "cwd=${_cwd}" \
        "commits_ahead=${_commits_ahead}"
      echo "git-queue: ERROR — branch mismatch (cross-role contamination guard, #2580)" >&2
      echo "  role:           ${ROLE}" >&2
      echo "  expected prefix: ${ROLE}/" >&2
      echo "  actual branch:   ${actual_branch}" >&2
      echo "  cwd:             ${_cwd}" >&2
      echo "" >&2
      echo "  Self-recovery (preserves your in-flight changes):" >&2
      echo "    git stash" >&2
      echo "    git checkout -b ${ROLE}/<card-id>" >&2
      echo "    git stash pop" >&2
      echo "" >&2
      echo "  Or, if you have an in-flight branch already:" >&2
      echo "    git stash; git checkout ${ROLE}/<existing-card-id>; git stash pop" >&2
      echo "" >&2
      echo "  Emergency override (logs the bypass, use sparingly):" >&2
      echo "    bash $0 ${op} --force-branch <args>" >&2
      return 1
  fi
}

log_event() {
  local event="$1"; shift
  "$CHORUS_LOG" "$event" "$ROLE" "$@" >/dev/null 2>/dev/null || true
}

# #2876: derive card_id from current branch (ROLE/CARD_ID convention) so the
# chorus-log env-bridge stamps build.* / commit.* events with card_id. Without
# this, build events drop out of chorus_logs_for_card joins and the pipeline-
# health report (#2874) cannot stitch demo->build->deploy stages together.
export_card_id_from_branch() {
  local _branch
  _branch=$(git -C "$REPO_ROOT" symbolic-ref --short HEAD 2>/dev/null || echo "")
  if [[ "$_branch" =~ ^[a-z]+/([0-9]+)$ ]]; then
    export CHORUS_CARD_ID="${BASH_REMATCH[1]}"
  fi
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
  rebase [--onto <ref>] [--accept-theirs <path>]
                                      Rebase current branch onto upstream (default origin/main).
                                      --accept-theirs auto-resolves conflicts on listed paths
                                      by accepting the upstream version (repeat for multiple paths).
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
  # #2580: parse --force-branch escape hatch before file/arg split
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi
  # #2731: --no-add skips the `git add` step for cases where the index is
  # already arranged exactly as the commit should look (e.g., untracking a
  # generated artifact via `git rm --cached` — `git add <ignored-path>` would
  # either reject or re-add from working tree, both wrong; the right move is
  # to commit the staged deletion as-is).
  local skip_add=false
  if [ "${1:-}" = "--no-add" ]; then
    skip_add=true
    shift
  fi
  if ! check_branch "commit" "$force_flag"; then
    exit 1
  fi

  # #2876: stamp card_id on subsequent build.* events
  export_card_id_from_branch

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
  # #2752 — capture git's stdout so it routes to stderr on failure. Git's
  # "nothing to commit, working tree clean" message goes to stdout even when
  # exit code is non-zero (longstanding git quirk). Without redirect,
  # downstream consumers (chorus_acp idempotent path, future tools) inspect
  # stderr and never see git's actual error message. Same family as #2700
  # (drop 2>&1 from do_pull/do_push) — getting error semantics right for MCP
  # classifiers at the wrapper layer.
  local exit_code=0
  local _git_stdout=""
  if $skip_add; then
    # #2777 — when --no-add is set, do NOT pass `-- <files>` pathspec to
    # `git commit`. Git's pathspec filtering respects .gitignore: a path
    # that is in .gitignore is silently dropped from the pathspec, even
    # if the index has it staged for deletion. Since --no-add exists for
    # exactly the staged-delete-of-now-ignored case, passing pathspec
    # negates the entire feature (#2731 latent bug, found while shipping
    # #2777 gitignore cleanup). The index has been arranged exactly as
    # the commit should look; commit-without-pathspec commits it as-is.
    # `files` is still required upstream (line 244) for log_event audit
    # context and is intentionally not threaded into the commit args here.
    _git_stdout=$(git commit "${git_args[@]}" 9>&-) || exit_code=$?
  else
    _git_stdout=$(git add "${files[@]}" 9>&- && git commit "${git_args[@]}" -- "${files[@]}" 9>&-) || exit_code=$?
  fi
  if [ $exit_code -eq 0 ]; then
    # Success: pass git's stdout through unchanged
    [ -n "$_git_stdout" ] && printf '%s\n' "$_git_stdout"
  else
    # Failure: send git's stdout to stderr so consumers see the real message
    [ -n "$_git_stdout" ] && printf '%s\n' "$_git_stdout" >&2
  fi

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
  # #2877: position-independent flag parsing. Pre-#2877 each flag (--force-branch,
  # --force-with-lease, --branch, --delete) was checked only at $1 and consumed
  # in fixed order. With the documented `push --branch X --force-with-lease`
  # usage, --force-with-lease silently never engaged because $1 was --branch.
  # Receipts: kade/2844 squash 2026-05-10 — non-FF rejection despite passing
  # --force-with-lease, which silently never engaged.
  local force_flag=""
  local force_with_lease=""
  local push_branch=""
  local delete_branch=""
  local allow_stale_base_deletions="false"
  while [ $# -gt 0 ]; do
    case "${1:-}" in
      --allow-stale-base-deletions)
        # #2944 → #3026: this was the override for the stale-base hard-block.
        # The block is now a non-fatal NOTE (#3026 — the squash-merge keeps the
        # files, so it was never a real revert), so this flag now only SUPPRESSES
        # the note. Kept for backward-compat with callers that still pass it.
        allow_stale_base_deletions="true"
        shift
        ;;
      --force-branch)
        force_flag="--force-branch"
        shift
        ;;
      --force-with-lease)
        # The safe force variant: refuses if remote ref changed since last
        # fetch (concurrent peer push detected) and pushes our rebased history
        # otherwise. Caller (chorus_commit, chorus_acp) opts in; raw operator
        # callers may opt in too. (#2799)
        #
        # WHEN to use --force-with-lease: post-rebase-recovery only — when local
        # history was rewritten (rebase, squash, fixup) and origin diverges as
        # a result. NOT a generic "my push is stuck" fix; if a plain push is
        # rejected for any other reason, force-with-lease will mask the real
        # cause. Wren feedback on #2877 — same discipline shape as the rebase-
        # cleanup comment in #2789.
        force_with_lease="--force-with-lease"
        shift
        ;;
      --branch)
        # Explicit push target. Replaces the _CHORUS_PUSH_REF env-carry
        # shipped in #2699. (#2705)
        shift
        push_branch="${1:-}"
        if [ -z "$push_branch" ]; then
          echo "git-queue: error — push --branch requires <ref>" >&2
          return 1
        fi
        shift
        ;;
      --delete)
        # Remote-only deletion path: no commit lands, no working-tree
        # interaction. Dispatched after parsing so flag order doesn't
        # matter. (#2701)
        shift
        delete_branch="${1:-}"
        shift || true
        ;;
      *)
        break
        ;;
    esac
  done

  if [ -n "$delete_branch" ]; then
    do_delete_remote "$delete_branch"
    return $?
  fi
  if ! check_branch "push" "$force_flag"; then
    exit 1
  fi

  # #2944 / #3026 — stale-base detection (NOTE, not block).
  #
  # #2944's premise: a stale werk branch silently deletes peer-merged files
  # because git's 3-way merge drops "present in main, absent in branch". #3026
  # corrected that: push targets the role branch, then /acp squash-merges,
  # applying ONLY the branch's own changeset — so a peer-added file absent from
  # a stale branch SURVIVES the squash (verified empirically). The detected
  # "deletions" are 2-way-diff artifacts, not real reverts; hard-blocking on
  # them jammed the whole team (2026-05-21).
  #
  # So we still DETECT (compute files deleted vs current main that the branch's
  # own commits never touched — stale-base ghosts, -M so renames don't count)
  # but only to NOTE them, so the operator can rebase for clean history. The
  # push always proceeds. The --allow-stale-base-deletions flag now just
  # suppresses the note.
  if [ "$allow_stale_base_deletions" != "true" ]; then
    git -C "$REPO_ROOT" fetch --quiet origin main 9>&- 2>/dev/null || true
    local _merge_base
    _merge_base=$(git -C "$REPO_ROOT" merge-base HEAD origin/main 2>/dev/null || echo "")
    if [ -n "$_merge_base" ]; then
      local _branch_deletions _branch_touched _silent_deletions
      _branch_deletions=$(git -C "$REPO_ROOT" diff -M --diff-filter=D --name-only "origin/main..HEAD" 2>/dev/null | sort -u)
      _branch_touched=$(git -C "$REPO_ROOT" diff -M --name-only "$_merge_base..HEAD" 2>/dev/null | sort -u)
      # Files this branch is "deleting" against current main that the
      # branch's own commits never touched = stale-base ghosts.
      _silent_deletions=$(comm -23 <(printf '%s\n' "$_branch_deletions") <(printf '%s\n' "$_branch_touched") | grep -v '^$' || true)
      if [ -n "$_silent_deletions" ]; then
        local _count
        _count=$(printf '%s\n' "$_silent_deletions" | wc -l | tr -d ' ')
        # #3026 — DOWNGRADED from hard-block to a non-fatal note. The premise
        # ("git's 3-way merge silently drops these files") does NOT hold for the
        # actual workflow: push targets the role branch, then /acp squash-merges
        # it, which applies ONLY the branch's own changeset. Verified empirically
        # — a peer-added file absent from a stale branch SURVIVES the squash
        # (it's added on main's side, untouched on the branch's, so the 3-way
        # keeps it). These "deletions" are 2-way-diff artifacts, not real
        # reverts. Hard-blocking on them jammed the whole team for an hour
        # (2026-05-21) and blocked legitimate non-overlapping branches. Note it
        # so the operator can rebase for clean linear history if they want, but
        # never block the push — a stale base is not, by itself, a revert.
        echo "note: branch is stale vs origin/main — ${_count} peer-added file(s) are not in your base." >&2
        echo "      The squash-merge keeps them (no revert); rebase onto origin/main only if you want clean linear history." >&2
        log_event "git_queue.push.stale_base_noted" "count=${_count} files=$(printf '%s\n' $_silent_deletions | tr '\n' ',')"
      fi
    fi
  fi

  # #2876: stamp card_id on build.push.* / build.delete.* events
  export_card_id_from_branch

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: push timeout — lock held" >&2
    exit 75
  fi
  write_meta
  log_event "build.push.started"

  # #2598: signal pre-push hook that this push originated through git-queue.
  # The pre-push hook refuses raw `git push` (no marker) and accepts only
  # invocations marked here. Substrate-uniformity (Jeff 2026-04-29).
  export _GIT_QUEUE_PUSH=1

  local dirty
  # #2597: `grep -v '^?'` returns 1 when stdin is empty (clean tree); under
  # `set -euo pipefail` that propagates through the command-substitution and
  # silently terminates the script before any push happens. `|| true` tolerates
  # the no-match case so a clean tree just produces empty `dirty`.
  dirty=$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null | grep -v '^?' | head -1 || true)

  local stashed=false
  if [ -n "$dirty" ]; then
    git -C "$REPO_ROOT" stash --quiet 9>&- 2>/dev/null
    stashed=true
  fi

  local exit_code=0
  # #2651 (#2639 AC5 re-land): skip pull --rebase when there's no upstream
  # yet (new-branch first push). pull-before-push assumes the remote ref
  # exists; for a brand-new branch it errors with "no such ref was fetched"
  # and aborts the push. Originally claimed shipped in #2639 but the working
  # -tree edit was lost in a dirty-state revert before commit.
  local has_upstream
  has_upstream=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)
  # #2700: don't merge stderr into stdout. MCP classifier reads err.stderr to
  # label push-conflict (rebase failure) vs push-fail; merging collapsed both
  # into push-fail by hiding the rebase/conflict signature text.
  # #2699: target the captured branch explicitly (origin REF:REF) regardless
  # of current HEAD. Defensive against Mode-A bumps between chorus_commit's
  # capture and this push step.
  # #2705: now via --branch <ref> arg (parsed at top of do_push) instead of
  # _CHORUS_PUSH_REF env. Substrate-uniform with --force-branch shape.
  # #2877: pin the lease to origin's view of the branch BEFORE pull-rebase
  # fetches. Without this, _try_pull_rebase_with_autoresolve runs `git pull`
  # which fetches origin and updates refs/remotes/origin/<branch> to current.
  # The subsequent --force-with-lease check then sees a "fresh" view and
  # always approves the force — even if a peer pushed concurrently between
  # our capture and our push. Pinning to the pre-fetch SHA preserves the
  # lease's safety promise: refuse if origin moved since we last looked.
  local _lease_pin=""
  if [ -n "$force_with_lease" ] && [ -n "$push_branch" ]; then
    # #2881 — must use --verify --quiet. Plain `rev-parse <ref>` on a missing
    # ref writes the literal ref name to stdout AND exits 128; `2>/dev/null
    # || echo ""` masks stderr+exit but the literal already flowed to stdout,
    # corrupting _lease_pin into a non-empty string and producing the malformed
    # flag `--force-with-lease=<branch>:refs/remotes/origin/<branch>`. With
    # --verify --quiet, missing refs return empty stdout cleanly (exit 1).
    _lease_pin=$(git -C "$REPO_ROOT" rev-parse --verify --quiet "refs/remotes/origin/${push_branch}" 2>/dev/null || echo "")
  fi
  # #2799: assemble push args with optional --force-with-lease. Force-with-lease
  # only applies on the rebase-then-push paths (has_upstream); fresh-branch
  # first push doesn't need it (no remote ref to overwrite). Argument
  # placement matters for git: flag before refspec.
  local _fwl_args=()
  if [ -n "$force_with_lease" ]; then
    if [ -n "$_lease_pin" ]; then
      # Pinned form: --force-with-lease=<refname>:<expected-sha>. Refuses if
      # origin's <refname> no longer equals <expected-sha>. Survives the
      # implicit fetch in pull-rebase (#2877).
      _fwl_args+=("--force-with-lease=${push_branch}:${_lease_pin}")
    else
      # Fresh branch (no origin ref to capture) — plain flag is sufficient
      # because there's nothing to clobber.
      _fwl_args+=("--force-with-lease")
    fi
  fi

  # #2865 — auto-resolve rebase conflicts on auto-generated files.
  # claudemd-gen auto-bumps designing/claudemd/manifest.json's _build integer
  # on every commit; branches always lag main; every push conflicts on this
  # file alone. When EVERY unmerged file is in the auto-generated allowlist,
  # take origin's version (rebase semantics: --ours == upstream side), stage,
  # continue. If even one conflicted file is human-edited, abort the rebase
  # so the caller sees an honest push-conflict and resolves manually.
  _autoresolve_rebase_drift() {
    local unmerged
    unmerged=$(git -C "$REPO_ROOT" diff --name-only --diff-filter=U 2>/dev/null)
    [ -z "$unmerged" ] && return 1
    while IFS= read -r f; do
      case "$f" in
        designing/claudemd/manifest.json) ;;
        *) return 1 ;;
      esac
    done <<< "$unmerged"
    while IFS= read -r f; do
      git -C "$REPO_ROOT" checkout --ours -- "$f" 9>&- 2>/dev/null || return 1
      git -C "$REPO_ROOT" add -- "$f" 9>&- 2>/dev/null || return 1
    done <<< "$unmerged"
    GIT_EDITOR=true git -C "$REPO_ROOT" rebase --continue 9>&- 2>/dev/null || return 1
    log_event "build.push.auto_resolved_rebase_drift" "files=$(echo "$unmerged" | tr '\n' ',')"
    return 0
  }

  _try_pull_rebase_with_autoresolve() {
    if git -C "$REPO_ROOT" pull --rebase 9>&-; then
      return 0
    fi
    if _autoresolve_rebase_drift; then
      return 0
    fi
    git -C "$REPO_ROOT" rebase --abort 9>&- 2>/dev/null || true
    return 1
  }

  # #2915 — packed-refs bounded retry (Silas condition 3a). Worktrees share
  # one .git/; concurrent ref operations (peer pushes, gc, chorus-werk
  # add/remove) collide on .git/packed-refs.lock and the push fails
  # transiently with "cannot lock ref" / "packed-refs.lock: File exists".
  # Retry with backoff before surfacing it as a real push-conflict.
  _git_push_retry() {
    local attempt=1 max=4 out rc
    while :; do
      rc=0
      out=$(git -C "$REPO_ROOT" push 9>&- "$@" 2>&1) || rc=$?
      if [ "$rc" -eq 0 ]; then
        if [ -n "$out" ]; then echo "$out" >&2; fi
        return 0
      fi
      if [ "$attempt" -lt "$max" ] && printf '%s' "$out" | grep -qE 'packed-refs\.lock|cannot lock ref'; then
        log_event "build.push.packed_refs_retry" "attempt=${attempt}"
        sleep "$(awk "BEGIN{print 0.2 * ${attempt}}")"
        attempt=$((attempt + 1))
        continue
      fi
      echo "$out" >&2
      return "$rc"
    done
  }

  if [ -n "$push_branch" ]; then
    if [ -z "$has_upstream" ]; then
      _git_push_retry origin "${push_branch}:${push_branch}" || exit_code=$?
    else
      _try_pull_rebase_with_autoresolve && _git_push_retry ${_fwl_args[@]+"${_fwl_args[@]}"} origin "${push_branch}:${push_branch}" || exit_code=$?
    fi
  else
    if [ -z "$has_upstream" ]; then
      _git_push_retry || exit_code=$?
    else
      _try_pull_rebase_with_autoresolve && _git_push_retry ${_fwl_args[@]+"${_fwl_args[@]}"} || exit_code=$?
    fi
  fi

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
  # #2877: emit force_with_lease field so spine analytics can distinguish
  # plain pushes from rewrite-pushes and trace lease-pin coverage over time.
  local _fwl_field="false"
  [ -n "$force_with_lease" ] && _fwl_field="true"
  local _fwl_pinned="false"
  [ -n "$_lease_pin" ] && _fwl_pinned="true"
  log_event "build.push.completed" "exit_code=${exit_code}" "force_with_lease=${_fwl_field}" "lease_pinned=${_fwl_pinned}"

  if [ $exit_code -ne 0 ]; then
    echo "git-queue: push failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Delete remote branch (#2701) ---
# Typed remote-branch deletion via the canonical adapter so cleanup ops don't
# need a heredoc bypass through chorus-hooks. No lock (no working-tree change),
# no rebase, no stash. Sets _GIT_QUEUE_PUSH=1 so the pre-push hook accepts.
do_delete_remote() {
  local branch="${1:-}"
  if [ -z "$branch" ]; then
    echo "git-queue: error — push --delete requires <branch>" >&2
    echo "  Usage: git-queue.sh push --delete <branch>" >&2
    return 1
  fi

  # Verify branch exists on remote — no destructive force, fail clean.
  if ! git -C "$REPO_ROOT" ls-remote --heads origin "$branch" 2>/dev/null | grep -q .; then
    echo "git-queue: error — branch '${branch}' does not exist on origin" >&2
    log_event "build.delete.skipped" "reason=missing" "branch=${branch}"
    return 1
  fi

  # Warn if branch has commits not reachable from origin/main — caller may
  # legitimately want to drop unmerged work, but it's worth surfacing.
  local main_ref="origin/main"
  if ! git -C "$REPO_ROOT" rev-parse --verify "$main_ref" >/dev/null 2>&1; then
    main_ref="origin/master"
  fi
  if git -C "$REPO_ROOT" rev-parse --verify "$main_ref" >/dev/null 2>&1; then
    local unique_count
    unique_count=$(git -C "$REPO_ROOT" rev-list --count "${main_ref}..origin/${branch}" 2>/dev/null || echo "0")
    if [ "${unique_count:-0}" -gt 0 ]; then
      echo "git-queue: WARNING — origin/${branch} has ${unique_count} commit(s) not reachable from ${main_ref}; deleting anyway" >&2
    fi
  fi

  # #2598: marker so pre-push hook accepts the invocation.
  export _GIT_QUEUE_PUSH=1
  log_event "build.delete.started" "branch=${branch}"

  local exit_code=0
  git -C "$REPO_ROOT" push origin --delete "$branch" || exit_code=$?

  if [ $exit_code -eq 0 ]; then
    log_event "branch.deleted" "branch=${branch}" "remote=origin"
    echo "git-queue: deleted origin/${branch}"
  else
    log_event "build.delete.failed" "branch=${branch}" "exit_code=${exit_code}"
    echo "git-queue: delete failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Pull with explicit dirty-tree surface (#2688) ---
# Sister to do_push but inverted on stash: do_push auto-stashes to keep push
# succeeding; do_pull lets dirty-tree fire honestly so caller recovers.
# On rebase conflict: abort cleanly to pre-rebase state, exit 1 with conflict
# stderr surfaced. MCP layer (chorus_pull) classifies via stderr patterns.

do_pull() {
  # #2580: parse --force-branch escape hatch (mirror do_push)
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi

  # Optional --branch / --remote (MCP layer passes these). Defaults: current
  # HEAD branch + origin (git's own defaults).
  local branch_arg="" remote_arg=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --branch) branch_arg="$2"; shift 2 ;;
      --remote) remote_arg="$2"; shift 2 ;;
      *) shift ;;
    esac
  done

  if ! check_branch "pull" "$force_flag"; then
    exit 1
  fi

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: timeout after ${LOCK_TIMEOUT}s — another commit is holding the lock" >&2
    exit 75
  fi
  write_meta
  log_event "build.pull.started"

  local exit_code=0
  # No auto-stash on pull — dirty-tree is a legitimate label the MCP layer
  # surfaces so callers know to commit/stash before retry. See #2688 AC2.
  local pull_args=("--rebase")
  if [ -n "$remote_arg" ]; then pull_args+=("$remote_arg"); fi
  if [ -n "$branch_arg" ]; then pull_args+=("$branch_arg"); fi
  # #2700: don't merge stderr into stdout. The MCP classifier reads err.stderr
  # to label refusals (rebase-conflict / dirty-tree / etc.); merging hid the
  # signature text and collapsed everything to pull-fail. The wrapper diagnostic
  # at line ~533 still echoes to stderr — operator readability preserved.
  git -C "$REPO_ROOT" pull "${pull_args[@]}" 9>&- || exit_code=$?

  # On rebase conflict, abort cleanly so the working tree returns to pre-rebase
  # state. The MCP layer emits chorus_pull.rebase.aborted; the user retries
  # without manual `git rebase --abort`.
  if [ $exit_code -ne 0 ]; then
    if git -C "$REPO_ROOT" rev-parse --git-path rebase-merge 2>/dev/null | xargs -I {} test -d {} 2>/dev/null \
       || git -C "$REPO_ROOT" rev-parse --git-path rebase-apply 2>/dev/null | xargs -I {} test -d {} 2>/dev/null; then
      git -C "$REPO_ROOT" rebase --abort 2>/dev/null || true
    fi
  fi

  clear_meta
  log_event "build.pull.completed" "exit_code=${exit_code}"

  if [ $exit_code -ne 0 ]; then
    echo "git-queue: pull failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Rebase (#2966) ---
# Closes the contradictory-hook stall: pre-push hook tells the agent to
# `git rebase origin/main`, but the PreToolUse chorus-hook-shim blocks
# raw `git rebase` from agent sessions (#2598 family). Without a sanctioned
# path, /acp gets stuck on stale-base silent-deletion refusals with no
# agent-accessible recovery. This subcommand provides the sanctioned path:
# same lock + branch-check + auto-abort discipline as do_pull, but rebases
# the current branch onto an upstream (default origin/main).
#
# Usage:
#   git-queue.sh rebase                            # rebase onto origin/main (default)
#   git-queue.sh rebase --onto <ref>               # rebase onto a specific ref
#   git-queue.sh rebase --accept-theirs <path>     # auto-resolve conflicts on this path
#                                                  # by accepting the upstream version
#                                                  # (repeat flag for multiple paths)
#
# --accept-theirs is the auto-regen escape hatch: when a rebase conflicts on
# a file that's always machine-generated (e.g. designing/claudemd/manifest.json)
# the upstream version is always correct and the local version is stale. Listing
# the path lets the rebase auto-resolve and continue instead of aborting. Loop
# is bounded — if a conflict appears on a path NOT in the accept-theirs list,
# the rebase aborts cleanly per the default contract.
#
# On unhandled conflict: aborts cleanly so the working tree returns to
# pre-rebase state. Caller resolves and retries (or extends --accept-theirs).
do_rebase() {
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi

  local onto_arg="origin/main"
  local accept_theirs_paths=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --onto) onto_arg="$2"; shift 2 ;;
      --accept-theirs) accept_theirs_paths+=("$2"); shift 2 ;;
      *) shift ;;
    esac
  done

  if ! check_branch "rebase" "$force_flag"; then
    exit 1
  fi

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: timeout after ${LOCK_TIMEOUT}s — another git op is holding the lock" >&2
    exit 75
  fi
  write_meta
  log_event "build.rebase.started" "onto=${onto_arg} accept_theirs_count=${#accept_theirs_paths[@]}"

  local fetch_exit=0
  git -C "$REPO_ROOT" fetch origin 9>&- 2>&1 | tail -3 || fetch_exit=$?
  if [ $fetch_exit -ne 0 ]; then
    clear_meta
    log_event "build.rebase.completed" "exit_code=${fetch_exit} stage=fetch"
    echo "git-queue: rebase aborted — fetch origin failed (exit ${fetch_exit})" >&2
    return $fetch_exit
  fi

  local exit_code=0
  git -C "$REPO_ROOT" rebase "$onto_arg" 9>&- || exit_code=$?

  # Auto-resolve loop: while rebase is paused on a conflict AND every
  # conflicted file is in --accept-theirs, checkout theirs + continue.
  # Bounded by max iterations to prevent infinite loops on weird states.
  local auto_resolve_iter=0
  local max_iter=50
  while [ $exit_code -ne 0 ] && [ $auto_resolve_iter -lt $max_iter ]; do
    # Are we mid-rebase?
    local rebase_dir
    rebase_dir=$(git -C "$REPO_ROOT" rev-parse --git-path rebase-merge 2>/dev/null)
    if [ ! -d "$rebase_dir" ]; then
      rebase_dir=$(git -C "$REPO_ROOT" rev-parse --git-path rebase-apply 2>/dev/null)
    fi
    [ -d "$rebase_dir" ] || break

    # If no --accept-theirs paths configured, no auto-resolve possible.
    [ ${#accept_theirs_paths[@]} -gt 0 ] || break

    # Get the conflicted files (unmerged paths).
    local conflicted
    conflicted=$(git -C "$REPO_ROOT" diff --name-only --diff-filter=U 2>/dev/null)
    [ -n "$conflicted" ] || break

    # Check each conflicted file against the accept-theirs list.
    local all_resolvable=1
    local resolved_count=0
    while IFS= read -r conflict_path; do
      [ -z "$conflict_path" ] && continue
      local matched=0
      for accept_path in "${accept_theirs_paths[@]}"; do
        if [ "$conflict_path" = "$accept_path" ]; then
          matched=1
          break
        fi
      done
      if [ $matched -eq 1 ]; then
        git -C "$REPO_ROOT" checkout --theirs -- "$conflict_path" 2>/dev/null
        git -C "$REPO_ROOT" add -- "$conflict_path" 2>/dev/null
        resolved_count=$((resolved_count + 1))
      else
        all_resolvable=0
        break
      fi
    done <<< "$conflicted"

    if [ $all_resolvable -eq 0 ]; then
      echo "git-queue: rebase conflict on unhandled path(s) — auto-resolve cannot continue" >&2
      break
    fi

    echo "git-queue: auto-resolved ${resolved_count} conflict(s) via --accept-theirs; continuing rebase" >&2
    log_event "build.rebase.auto_resolved" "count=${resolved_count}"
    exit_code=0
    git -C "$REPO_ROOT" -c core.editor=true rebase --continue 9>&- || exit_code=$?
    auto_resolve_iter=$((auto_resolve_iter + 1))
  done

  # Final cleanup: if rebase is still mid-flight (unhandled conflict or
  # iter limit), abort cleanly per the default contract.
  if [ $exit_code -ne 0 ]; then
    if git -C "$REPO_ROOT" rev-parse --git-path rebase-merge 2>/dev/null | xargs -I {} test -d {} 2>/dev/null \
       || git -C "$REPO_ROOT" rev-parse --git-path rebase-apply 2>/dev/null | xargs -I {} test -d {} 2>/dev/null; then
      git -C "$REPO_ROOT" rebase --abort 2>/dev/null || true
      echo "git-queue: rebase conflict detected onto ${onto_arg} — aborted cleanly. Resolve manually or extend --accept-theirs and retry." >&2
    fi
  fi

  clear_meta
  log_event "build.rebase.completed" "exit_code=${exit_code} onto=${onto_arg} auto_resolve_iter=${auto_resolve_iter}"

  if [ $exit_code -eq 0 ]; then
    echo "git-queue: rebased onto ${onto_arg} (auto-resolved ${auto_resolve_iter} conflict round(s))" >&2
  else
    echo "git-queue: rebase failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Checkout / switch / branch (#2710) ---
# Candidate A from #2706 Mode-A close: serialize working-tree mutation through
# the canonical adapter so concurrent peers can't bump HEAD mid-session-read.
# Mirrors do_pull's shape: lock + check_branch (with --force-branch escape) +
# stderr-preserving wrapper diagnostic + spine event taxonomy.
do_checkout() {
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi
  if ! check_branch "checkout" "$force_flag"; then
    exit 1
  fi
  if [ $# -eq 0 ]; then
    echo "git-queue: error — checkout requires <branch> or args" >&2
    return 1
  fi

  # Already-on-branch no-op: if the only arg is the current branch name, skip.
  if [ $# -eq 1 ]; then
    local current
    current=$(git -C "$REPO_ROOT" symbolic-ref --short HEAD 2>/dev/null || echo "")
    if [ -n "$current" ] && [ "$1" = "$current" ]; then
      return 0
    fi
  fi

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: checkout timeout — lock held" >&2
    exit 75
  fi
  write_meta
  log_event "build.checkout.started" "args=$*"

  local exit_code=0
  git -C "$REPO_ROOT" checkout "$@" 9>&- || exit_code=$?

  clear_meta
  if [ $exit_code -eq 0 ]; then
    log_event "build.checkout.completed" "args=$*"
  else
    log_event "build.checkout.failed" "exit_code=${exit_code}" "args=$*"
    echo "git-queue: checkout failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

do_switch() {
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi
  if ! check_branch "switch" "$force_flag"; then
    exit 1
  fi
  if [ $# -eq 0 ]; then
    echo "git-queue: error — switch requires <branch>" >&2
    return 1
  fi

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: switch timeout — lock held" >&2
    exit 75
  fi
  write_meta
  log_event "build.checkout.started" "op=switch" "args=$*"

  local exit_code=0
  git -C "$REPO_ROOT" switch "$@" 9>&- || exit_code=$?

  clear_meta
  if [ $exit_code -eq 0 ]; then
    log_event "build.checkout.completed" "op=switch" "args=$*"
  else
    log_event "build.checkout.failed" "op=switch" "exit_code=${exit_code}" "args=$*"
    echo "git-queue: switch failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

do_branch() {
  local force_flag=""
  if [ "${1:-}" = "--force-branch" ]; then
    force_flag="--force-branch"
    shift
  fi
  if ! check_branch "branch" "$force_flag"; then
    exit 1
  fi
  if [ $# -eq 0 ]; then
    echo "git-queue: error — branch requires <name>" >&2
    return 1
  fi

  exec 9>"$LOCK_FILE"
  if ! lockf -t "$LOCK_TIMEOUT" 9; then
    echo "git-queue: branch timeout — lock held" >&2
    exit 75
  fi
  write_meta
  log_event "build.checkout.started" "op=branch" "args=$*"

  local exit_code=0
  git -C "$REPO_ROOT" branch "$@" 9>&- || exit_code=$?

  clear_meta
  if [ $exit_code -eq 0 ]; then
    log_event "build.checkout.completed" "op=branch" "args=$*"
  else
    log_event "build.checkout.failed" "op=branch" "exit_code=${exit_code}" "args=$*"
    echo "git-queue: branch failed (exit ${exit_code})" >&2
  fi
  return $exit_code
}

# --- Main ---

cmd="${1:-help}"
shift || true

case "$cmd" in
  commit)   do_commit "$@" ;;
  push)     do_push "$@" ;;
  pull)     do_pull "$@" ;;
  rebase)   do_rebase "$@" ;;
  checkout) do_checkout "$@" ;;
  switch)   do_switch "$@" ;;
  branch)   do_branch "$@" ;;
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
