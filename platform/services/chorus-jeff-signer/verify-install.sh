#!/bin/bash
# #4652 — read-only check that Jeff's signer install is a real wall. Run by a role
# AFTER Jeff's step 4. Every line must read PASS; it never reads the key itself.
U=chorus-jeff-signer; K=/Users/$U/.signer/key; B=/usr/local/libexec/chorus-jeff-signer; L=/Library/Logs/chorus-jeff-signer.log
ok=0; bad=0; check() { if eval "$2"; then echo "PASS $1"; ok=$((ok+1)); else echo "FAIL $1"; bad=$((bad+1)); fi; }
check "the user exists"                          "id -u $U >/dev/null 2>&1"
# Each refusal check first requires the thing to exist, so a missing install
# can never read PASS (#3734: a guard must not pass vacuously).
H=/Users/$U
check "a role cannot sudo to it without a password" "id -u $U >/dev/null 2>&1 && ! sudo -n -u $U true 2>/dev/null"
check "its home is $U's own, mode 700"           "[ -d $H ] && [ \"\$(stat -f %Su $H)\" = $U ] && [ \"\$(stat -f %Lp $H)\" = 700 ]"
check "a role cannot list its home"              "[ -d $H ] && ! ls $H >/dev/null 2>&1"
check "a role cannot read the key"               "[ -d $H ] && ! head -c1 $K >/dev/null 2>&1"
check "the binary is root-owned and not writable by us" "[ \"\$(stat -f %Su $B)\" = root ] && [ ! -w $B ]"
check "its folder is root-owned"                 "[ \"\$(stat -f %Su \$(dirname $B))\" = root ]"
check "the daemon is loaded"                     "launchctl print system/com.chorus.jeff-signer >/dev/null 2>&1"
check "it runs as $U"                            "ps -axo user=,comm= | grep -q \"^$U .*chorus-jeff-signer\""
check "the log shows it authenticated"           "grep -q 'authenticated, listening' $L"
echo "$ok pass · $bad fail"; [ $bad -eq 0 ]
