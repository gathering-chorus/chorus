#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# infrastructure-service-design.bats — Tests for #2299
# What Jeff sees: one page showing the full home cloud topology.

PAGE="http://localhost:3000/gathering-docs/infrastructure-service-design.html"
FILE="${HOME}/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/infrastructure-service-design.html"

@test "AC1: page exists and serves 200" {
  run curl -sf -o /dev/null -w "%{http_code}" "$PAGE"
  [ "$output" = "200" ]
}

@test "AC2: has Compute section with both Macs" {
  run curl -sf "$PAGE"
  [[ "$output" == *"Library Mac"* ]]
  [[ "$output" == *"Bedroom Mac"* ]]
  [[ "$output" == *"192.168.86.36"* ]]
  [[ "$output" == *"192.168.86.242"* ]]
}

@test "AC2: has Network section" {
  run curl -sf "$PAGE"
  [[ "$output" == *"Home Mesh"* ]]
  [[ "$output" == *"Nest WiFi"* ]]
}

@test "AC2: has Storage section" {
  run curl -sf "$PAGE"
  [[ "$output" == *"Library Storage"* ]]
  [[ "$output" == *"Bedroom Storage"* ]]
  [[ "$output" == *"APFS"* ]]
}

@test "AC2: has Services section" {
  run curl -sf "$PAGE"
  [[ "$output" == *"Service Lifecycle"* ]]
  [[ "$output" == *"app-state.sh"* ]]
}

@test "AC2: has Backup section" {
  run curl -sf "$PAGE"
  [[ "$output" == *"Backup"* ]]
  [[ "$output" == *"Time Machine"* ]]
}

@test "AC2: has Constraints section" {
  run curl -sf "$PAGE"
  [[ "$output" == *"C1"* ]]
  [[ "$output" == *"C5"* ]]
  [[ "$output" == *"C7"* ]]
}

@test "AC3: shows real IPs, ports, service names" {
  run curl -sf "$PAGE"
  [[ "$output" == *"3000"* ]]
  [[ "$output" == *"3030"* ]]
  [[ "$output" == *"3100"* ]]
  [[ "$output" == *"3340"* ]]
}

@test "AC4: gaps called out per section" {
  content=$(curl -sf "$PAGE")
  gap_count=$(echo "$content" | grep -c "class=\"gap\"")
  [ "$gap_count" -ge 3 ]
}

@test "AC5: links to observability service design" {
  run curl -sf "$PAGE"
  [[ "$output" == *"observability-service-design.html"* ]]
}
