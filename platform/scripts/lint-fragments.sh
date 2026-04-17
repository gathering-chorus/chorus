#!/usr/bin/env bash
# lint-fragments.sh — fitness linter for CLAUDE.md fragment system (#2150)
#
# Rules:
#   R1 PRINCIPLE_DRIFT   same principle name, divergent wording across roles
#   R2 DUPLICATION       parallel fragment Jaccard >=80% (consolidation candidate)
#   R3 STALE             shared fragment unreferenced beyond tier threshold
#                        (shared-infra 90d / operating-norms 30d / principles never)
#   R4 ASYMMETRY         parallel fragment present for 2+ roles but missing for 1
#   R5 DANGLING_DEC      DEC-### citation not in decisions.md
#   R6 SIZE_VARIANCE     parallel fragment line-count variance >50% (warning)
#
# Exit codes: 0 clean, 1 warnings only (R6), 2 errors (R1/R2/R3/R4/R5).

set -u

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FIXTURE="$CHORUS_ROOT/designing/claudemd"
DECISIONS_FILE="$CHORUS_ROOT/roles/wren/decisions.md"
JSON_OUT=0
ACTIVITY_FILE="$CHORUS_ROOT/activity.md"
SESSIONS_DIR="$CHORUS_ROOT/roles"

while [ $# -gt 0 ]; do
  case "$1" in
    --fixture) FIXTURE="$2"; shift 2 ;;
    --decisions) DECISIONS_FILE="$2"; shift 2 ;;
    --json) JSON_OUT=1; shift ;;
    --help|-h)
      grep '^#' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

set +u  # FINDINGS[@] unset-check tolerance on bash 3.2
FINDINGS=()
STATUS=0  # escalates: 0 → 1 (warn) → 2 (error)

emit() {
  # emit <rule> <severity> <fragment> <detail>
  local rule="$1" sev="$2" frag="$3" detail="$4"
  FINDINGS+=("$rule|$sev|$frag|$detail")
  if [ "$sev" = "error" ]; then STATUS=2
  elif [ "$sev" = "warn" ] && [ "$STATUS" -lt 1 ]; then STATUS=1
  fi
}

roles_with_fragment() {
  local frag="$1" out=()
  for role in wren silas kade; do
    if [ -f "$FIXTURE/roles/$role/$frag" ]; then out+=("$role"); fi
  done
  printf "%s\n" "${out[@]}"
}

# Normalize content for similarity: strip markdown structure, lowercase, split into tokens.
# Silas's adjustment: bare hash would miss "- foo" vs "foo." — strip markdown first.
tokenize() {
  local file="$1"
  sed -E '
    s/^[[:space:]]*#{1,6}[[:space:]]*//g;   # heading markers
    s/^[[:space:]]*[-*+][[:space:]]+//g;     # list markers
    s/^[[:space:]]*[0-9]+\.[[:space:]]+//g;  # numbered list
    s/\*\*([^*]+)\*\*/\1/g;                  # bold
    s/\*([^*]+)\*/\1/g;                      # italic
    s/`([^`]+)`/\1/g;                        # code spans
    s/\[([^]]+)\]\([^)]+\)/\1/g;             # links
    s/[[:punct:]]/ /g;                       # remaining punctuation
  ' "$file" | tr '[:upper:]' '[:lower:]' | tr -s '[:space:]' '\n' | sed '/^$/d' | sort -u
}

jaccard() {
  # jaccard <file-a> <file-b> → prints 0.00-1.00
  local a; a=$(mktemp); local b; b=$(mktemp)
  tokenize "$1" > "$a"
  tokenize "$2" > "$b"
  local ua; ua=$(sort -u "$a" "$b" | wc -l | tr -d ' ')
  local ia; ia=$(comm -12 "$a" "$b" | wc -l | tr -d ' ')
  rm -f "$a" "$b"
  if [ "$ua" -eq 0 ]; then echo "0.00"; return; fi
  awk -v i="$ia" -v u="$ua" 'BEGIN { printf "%.2f", i/u }'
}

# R4 — collect parallel fragments (file name present in roles/{wren,silas,kade}/)
# Bash 3.2 compat: no associative arrays; use sorted -u list of basenames.
all_names=""
for role in wren silas kade; do
  if [ -d "$FIXTURE/roles/$role" ]; then
    for f in "$FIXTURE/roles/$role"/*.md; do
      [ -f "$f" ] || continue
      all_names="$all_names$(basename "$f")
"
    done
  fi
done
unique_names=$(printf "%s" "$all_names" | sort -u | sed '/^$/d')

while IFS= read -r name; do
  [ -z "$name" ] && continue
  present=()
  missing=()
  for role in wren silas kade; do
    if [ -f "$FIXTURE/roles/$role/$name" ]; then
      present+=("$role")
    else
      missing+=("$role")
    fi
  done
  # R4 asymmetry: present for ≥2, missing for ≥1
  if [ "${#present[@]}" -ge 2 ] && [ "${#missing[@]}" -ge 1 ]; then
    emit "R4" "error" "$name" "present in ${present[*]}; missing in ${missing[*]}"
  fi

  # R2 duplication (only if present in ≥2 roles)
  if [ "${#present[@]}" -ge 2 ]; then
    max_sim="0.00"
    worst_pair=""
    for ((i=0; i<${#present[@]}; i++)); do
      for ((j=i+1; j<${#present[@]}; j++)); do
        a="$FIXTURE/roles/${present[i]}/$name"
        b="$FIXTURE/roles/${present[j]}/$name"
        s=$(jaccard "$a" "$b")
        if awk -v s="$s" -v m="$max_sim" 'BEGIN { exit !(s > m) }'; then
          max_sim="$s"; worst_pair="${present[i]}+${present[j]}"
        fi
      done
    done
    if awk -v s="$max_sim" 'BEGIN { exit !(s >= 0.80) }'; then
      emit "R2" "error" "$name" "jaccard=$max_sim across $worst_pair (≥0.80 → consolidation candidate)"
    fi
  fi

  # R6 size variance (only if present in ≥2 roles)
  if [ "${#present[@]}" -ge 2 ]; then
    min_lines=999999; max_lines=0
    for role in "${present[@]}"; do
      lc=$(wc -l < "$FIXTURE/roles/$role/$name" | tr -d ' ')
      [ "$lc" -lt "$min_lines" ] && min_lines="$lc"
      [ "$lc" -gt "$max_lines" ] && max_lines="$lc"
    done
    if [ "$max_lines" -gt 0 ]; then
      variance=$(awk -v mn="$min_lines" -v mx="$max_lines" 'BEGIN { printf "%.2f", (mx-mn)/mx }')
      if awk -v v="$variance" 'BEGIN { exit !(v > 0.50) }'; then
        emit "R6" "warn" "$name" "line-count variance=$variance (min=$min_lines max=$max_lines)"
      fi
    fi
  fi
done <<< "$unique_names"

# R1 — principle drift: same leading phrase across roles' principles.md with divergent full wording
if [ -f "$FIXTURE/roles/wren/principles.md" ] && \
   [ -f "$FIXTURE/roles/silas/principles.md" ] && \
   [ -f "$FIXTURE/roles/kade/principles.md" ]; then
  # Extract bullet items, take first 3 tokens as "principle name", compare remainder
  tmpdir=$(mktemp -d)
  for role in wren silas kade; do
    grep -E '^[[:space:]]*[-*+] ' "$FIXTURE/roles/$role/principles.md" 2>/dev/null | \
      sed -E 's/^[[:space:]]*[-*+] //' | \
      awk '{ key=""; for(i=1;i<=3 && i<=NF;i++) key = key " " $i; print key "|" $0 }' \
      > "$tmpdir/$role.principles"
  done
  # Find keys present in all 3
  awk -F'|' '{ print $1 }' "$tmpdir/wren.principles" | sort -u > "$tmpdir/keys.wren"
  awk -F'|' '{ print $1 }' "$tmpdir/silas.principles" | sort -u > "$tmpdir/keys.silas"
  awk -F'|' '{ print $1 }' "$tmpdir/kade.principles" | sort -u > "$tmpdir/keys.kade"
  comm -12 "$tmpdir/keys.wren" "$tmpdir/keys.silas" | comm -12 - "$tmpdir/keys.kade" > "$tmpdir/shared.keys"
  while IFS= read -r key; do
    [ -z "$key" ] && continue
    w=$(grep -F "$key|" "$tmpdir/wren.principles" | head -1 | cut -d'|' -f2-)
    s=$(grep -F "$key|" "$tmpdir/silas.principles" | head -1 | cut -d'|' -f2-)
    k=$(grep -F "$key|" "$tmpdir/kade.principles" | head -1 | cut -d'|' -f2-)
    if [ "$w" != "$s" ] || [ "$s" != "$k" ]; then
      emit "R1" "error" "principles.md" "drift on principle starting '$key': divergent wording across roles"
    fi
  done < "$tmpdir/shared.keys"
  rm -rf "$tmpdir"
fi

# R5 — dangling DEC-### citations
if [ -f "$DECISIONS_FILE" ]; then
  known_decs=$(grep -oE 'DEC-[0-9]+' "$DECISIONS_FILE" 2>/dev/null | sort -u)
  # Scan all fragments
  for f in "$FIXTURE/shared"/*.md "$FIXTURE/roles"/*/*.md; do
    [ -f "$f" ] || continue
    cited=$(grep -oE 'DEC-[0-9]+' "$f" 2>/dev/null | sort -u)
    for dec in $cited; do
      if ! echo "$known_decs" | grep -qFx "$dec"; then
        emit "R5" "error" "$(basename "$f")" "cites $dec but not found in $(basename "$DECISIONS_FILE")"
      fi
    done
  done
fi

# R3 — stale shared fragment (tiered thresholds)
# classify_tier <filename> → prints "infra"|"norms"|"principles"
classify_tier() {
  case "$1" in
    infrastructure*|cross-machine*|domain-endpoints*|icd*|portfolio*) echo "infra" ;;
    *principle*) echo "principles" ;;
    *) echo "norms" ;;
  esac
}

if [ -d "$FIXTURE/shared" ]; then
  now=$(date +%s)
  for f in "$FIXTURE/shared"/*.md; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    tier=$(classify_tier "$name")
    [ "$tier" = "principles" ] && continue  # never stale per Wren call
    threshold_days=30
    [ "$tier" = "infra" ] && threshold_days=90
    mtime=$(stat -f %m "$f" 2>/dev/null || stat -c %Y "$f" 2>/dev/null)
    [ -z "$mtime" ] && continue
    age_days=$(( (now - mtime) / 86400 ))
    if [ "$age_days" -gt "$threshold_days" ]; then
      refs=0
      [ -f "$ACTIVITY_FILE" ] && refs=$(( refs + $(grep -c "$name" "$ACTIVITY_FILE" 2>/dev/null || echo 0) ))
      if [ "$refs" -eq 0 ]; then
        emit "R3" "error" "$name" "age=${age_days}d > ${threshold_days}d threshold (tier=$tier); no activity/briefs references"
      fi
    fi
  done
fi

# Output
if [ "$JSON_OUT" -eq 1 ]; then
  echo "["
  n=${#FINDINGS[@]}
  for ((i=0; i<n; i++)); do
    IFS='|' read -r rule sev frag detail <<< "${FINDINGS[i]}"
    comma=","
    [ $((i+1)) -eq "$n" ] && comma=""
    printf '  {"rule":"%s","severity":"%s","fragment":"%s","detail":"%s"}%s\n' \
      "$rule" "$sev" "$frag" "$(echo "$detail" | sed 's/"/\\"/g')" "$comma"
  done
  echo "]"
else
  for row in "${FINDINGS[@]}"; do
    IFS='|' read -r rule sev frag detail <<< "$row"
    printf "%s [%s] %s: %s\n" "$rule" "$sev" "$frag" "$detail"
  done
  if [ "${#FINDINGS[@]}" -eq 0 ]; then
    echo "lint-fragments: clean (0 findings)"
  else
    echo "---"
    echo "lint-fragments: ${#FINDINGS[@]} finding(s), status=$STATUS"
  fi
fi

exit "$STATUS"
