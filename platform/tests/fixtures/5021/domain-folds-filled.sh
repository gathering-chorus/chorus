#!/bin/sh
# @domain: policies
# #5021 — writes into $1 a stub of the endpoints the Athena domain page reads, for two domains:
# y fills every fold; x leaves Releases empty and gives no foldNote for it (one violation of
# domain-folds-filled), and leaves Infrastructure empty WITH a note (not a violation).
# Serve $1 with `python3 -m http.server` and point CHORUS_API at it.
set -e
d="$1"
mkdir -p "$d/owl/v1/domains/domains" "$d/owl/code" "$d/owl/logs" "$d/api/chorus/domain/x" "$d/api/chorus/domain/y"
for n in x y; do
  note=''; [ "$n" = x ] && note='"foldNote": ["Infrastructure: none of its own, it runs on Library"],'
  # #5045: x names no gap card and no design doc; neither is counted (the page writes its own
  # reason for those two folds), so x's one violation is still Releases alone.
  doc='"hasDesignDoc": "chorus:doc",'; card='"gapCard": "1",'
  [ "$n" = x ] && { doc=''; card=''; }
  cat > "$d/owl/v1/domains/domains/$n" <<J
{"data": {"comment": "c", $card $note "label": "$n"},
 "links": {"definesVocabulary": "chorus:T", $doc "hosts": "chorus:svc"}}
J
  printf '{"data": {"direct": {"consumes": ["chorus:time"], "consumedBy": []}}}' > "$d/api/chorus/domain/$n/dependencies"
  printf '{"domain": "%s", "cards": [{"id": "1"}]}' "$n" > "$d/api/chorus/domain/$n/cards"
  printf '{"data": {"tests": [{"path": "t"}]}}' > "$d/api/chorus/domain/$n/tests"
  printf '{"data": {"stages": [{"name": "s"}]}}' > "$d/api/chorus/domain/$n/pipeline"
  printf '{"data": {"decisions": [{"id": "adr-1"}]}}' > "$d/api/chorus/domain/$n/decisions"
  printf '{"data": {"alerts": [{"name": "a"}]}}' > "$d/api/chorus/domain/$n/alerts"
done
printf '{"data": {"releases": [{"cardId": "1"}]}}' > "$d/api/chorus/domain/y/releases"
printf '{"data": {"releases": []}}' > "$d/api/chorus/domain/x/releases"
printf '{"data": {"environments": [{"name": "prod"}]}}' > "$d/api/chorus/domain/y/infra"
printf '{"data": {"environments": []}}' > "$d/api/chorus/domain/x/infra"
for f in endpoints pages files; do
  printf '{"data": [{"hasDomain": "chorus:x"}, {"hasDomain": ["chorus:y"]}]}' > "$d/owl/code/$f"
done
printf '{"data": [{"links": {"hasDomain": "chorus:x"}}, {"hasDomain": "chorus:y"}]}' > "$d/owl/logs/sources"
