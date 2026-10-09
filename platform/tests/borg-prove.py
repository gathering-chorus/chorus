#!/usr/bin/env python3
# @test-type: unit
# @domain: borg
"""
Test suite for borg-prove with negative proof verification.
"""
import subprocess
import sys
import json
import os
import tempfile

def test_borg_prove():
    print("Running borg-prove tests...")
    
    chorus_root = os.environ.get("CHORUS_ROOT")
    if not chorus_root:
        chorus_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        
    script = os.path.join(chorus_root, "platform", "scripts", "borg-prove")
    
    # Test 1: Run on #4456 (4 unmeasured AC) with --prod
    res = subprocess.run(["python3", script, "--prod", "4456"], capture_output=True, text=True)
    print("--- #4456 run output ---")
    print(res.stdout)
    assert "UNMEASURED" in res.stdout, "Should report unmeasured for 4456"
    assert res.returncode == 0, "Should exit 0 when no explicit failures"
    
    # Test 2: Run on #4445 (6 unmeasured AC) with --demo
    res4445 = subprocess.run(["python3", script, "--demo", "4445"], capture_output=True, text=True)
    print("--- #4445 run output ---")
    print(res4445.stdout)
    assert "UNMEASURED" in res4445.stdout, "Should report unmeasured for 4445"
    assert res4445.returncode == 0, "Should exit 0 when no explicit failures"
    
    # Test 3: Negative proof fixture (card with an AC check that fails)
    fixture_dir = tempfile.mkdtemp()
    fixture_path = os.path.join(fixture_dir, "test-snapshot.json")
    fixture_data = {
        "tasks": [
            {
                "index": 9999,
                "title": "Negative proof test card",
                "description": "## AC\n- [ ] Passing check -- check: python3 -c 'import sys; sys.exit(0)'\n- [ ] Failing check -- check: python3 -c 'import sys; sys.exit(1)'"
            }
        ]
    }
    with open(fixture_path, "w") as f:
        json.dump(fixture_data, f)
        
    env = os.environ.copy()
    env["BORG_SNAPSHOT"] = fixture_path
    res_neg = subprocess.run(["python3", script, "--prod", "9999"], capture_output=True, text=True, env=env)
    print("--- Negative proof fixture output ---")
    print(res_neg.stdout)
    print(res_neg.stderr)
    
    assert "[MET]" in res_neg.stdout, "Passing check should be met ([MET])"
    assert "[NOT MET]" in res_neg.stdout, "Failing check should be not met ([NOT MET])"
    assert res_neg.returncode != 0, "Should exit non-zero when a check fails"
    
    print("All borg-prove tests and negative proof verification passed successfully!")

if __name__ == "__main__":
    test_borg_prove()
