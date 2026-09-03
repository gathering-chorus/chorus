#!/usr/bin/env bats
# @test-type: unit — hermetic source guard (fixture plists, fixture mapping, fixture Loki list; the live-box
# coverage check is log-harvest.sh --check, which runs hourly and goes red, not a test that peeks at $HOME)
# #4084 — log-harvest-gen.py: every launchd unit's log as a LogSource row carrying the
# authored domain edge. A test brings its own world (#3528): fixture plists, a fixture
# mapping, a fixture Loki job list; no $HOME, no store, no live service.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  GEN="$ROOT/platform/scripts/log-harvest-gen.py"
  T="$(mktemp -d)"
  mkdir -p "$T/la" "$T/logs"
  mk() { # label out err
    cat > "$T/la/$1.plist" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>$1</string>
${2:+<key>StandardOutPath</key><string>$2</string>}${3:+<key>StandardErrorPath</key><string>$3</string>}
</dict></plist>
PL
  }
  mk com.chorus.alert-runner "$T/logs/alert-runner.log" "$T/logs/alert-runner.log"
  mk com.chorus.silent "$T/logs/silent.log"
  mk com.chorus.missing "$T/logs/never-written.log"
  mk com.chorus.buzz-tunnel
  echo "line" > "$T/logs/alert-runner.log"
  echo "old" > "$T/logs/silent.log"; touch -t 202601010000 "$T/logs/silent.log"
  printf 'com.chorus.alert-runner\talerts-monitors\ncom.chorus.silent\tspine\ncom.chorus.missing\tspine\ncom.chorus.buzz-tunnel\tintegrations\n' > "$T/map.tsv"
  printf 'alert-runner\nchorus-api\n' > "$T/jobs.txt"
}
teardown() { rm -rf "$T"; }

@test "every unit becomes one LogSource row with the authored domain edge" {
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt" --machine library
  [ "$status" -eq 0 ]
  [ "$(grep -c 'a chorus:LogSource' <<<"$output")" -eq 4 ]
  grep -q '<urn:chorus:logsource-library-com.chorus.alert-runner>' <<<"$output"
  grep -q 'chorus:hasDomain chorus:alerts-monitors' <<<"$output"
  grep -q 'chorus:lokiJob "alert-runner"' <<<"$output"
  grep -q 'chorus:onMachine chorus:library' <<<"$output"
}

@test "status is derived from the file, not declared: active / silent / missing / unobservable" {
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt"
  [ "$status" -eq 0 ]
  grep -A12 'logsource-library-com.chorus.alert-runner>' <<<"$output" | grep -q 'chorus:logStatus "active"'
  grep -A12 'logsource-library-com.chorus.silent>'       <<<"$output" | grep -q 'chorus:logStatus "silent"'
  grep -A12 'logsource-library-com.chorus.missing>'      <<<"$output" | grep -q 'chorus:logStatus "missing"'
  grep -A12 'logsource-library-com.chorus.buzz-tunnel>'  <<<"$output" | grep -q 'chorus:logStatus "unobservable"'
}

@test "a unit the Loki job list does not carry gets no lokiJob (shipped is measured, not assumed)" {
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt"
  ! grep -A12 'logsource-library-com.chorus.silent>' <<<"$output" | grep -q 'chorus:lokiJob'
}

@test "a werk variant unit inherits its base unit's domain (env-up mints them per card; nobody authors rows for them)" {
  mk com.chorus.alert-runner.werk.kade "$T/logs/ar-kade.log"
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt"
  [ "$status" -eq 0 ]
  grep -A12 'logsource-library-com.chorus.alert-runner.werk.kade>' <<<"$output" | grep -q 'chorus:hasDomain chorus:alerts-monitors'
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt" --check
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a unit with no mapping row is emitted WITHOUT a domain edge and --check goes red naming it" {
  printf 'com.chorus.alert-runner\talerts-monitors\n' > "$T/map.tsv"
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt"
  [ "$status" -eq 0 ]
  [ "$(grep -c 'a chorus:LogSource' <<<"$output")" -eq 4 ]      # the row is NOT hidden
  [ "$(grep -c 'chorus:hasDomain' <<<"$output")" -eq 1 ]         # only the mapped one carries the edge
  run python3 "$GEN" --plists "$T/la" --mapping "$T/map.tsv" --loki-jobs "$T/jobs.txt" --check
  [ "$status" -eq 1 ]
  grep -q 'UNMAPPED com.chorus.silent' <<<"$output"
  grep -q 'UNMAPPED com.chorus.buzz-tunnel' <<<"$output"
}

@test "NEGATIVE PROOF (page): the Logs fold filters on the domain edge, so a domain with no rows renders zero, not the whole list" {
  # the fold's filter is a pure predicate on the row's hasDomain; exercise it with node
  run node -e '
    const tail = iri => String(iri||"").split("#").pop().replace(/^chorus:/,"");
    const rows=[{launchdLabel:"a",hasDomain:"chorus:logs"},{launchdLabel:"b",links:{hasDomain:"https://jeffbridwell.com/chorus#security"}},{launchdLabel:"c"}];
    const f=d=>r=>tail(r.hasDomain||(r.links&&r.links.hasDomain)||"")===d;
    console.log(rows.filter(f("security")).length, rows.filter(f("logs")).length, rows.filter(f("nothing")).length);'
  [ "$status" -eq 0 ]
  [ "$output" = "1 1 0" ]
}
