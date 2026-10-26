#!/bin/bash
# @test-type: unit — the replay's own negative proof; no services
# @domain: awareness
#
# #4744 AC9 (Jeff 10-26 09:53, via Kade): callers never break quietly. platform/scripts/pulse-tree-replay
# diffs the hand-written tree (:3475) against the generated one (:3392) for every role
# before the hand-written routes retire. Its --fixture mode proves each state it exists to
# catch is caught (a role's state, a lost prompt, a lost card) and that time drift is not.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
out="$(python3 "$here/../scripts/pulse-tree-replay" --fixture 2>&1)"
if [ "$out" = "NEGATIVE PROOF OK" ]; then
  echo "ok 1 - pulse-tree-replay catches every difference it exists to catch"
  exit 0
fi
echo "not ok 1 - pulse-tree-replay --fixture: $out"
exit 1
