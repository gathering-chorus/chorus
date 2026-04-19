# chorus-hooks build + deploy
# "cargo build" alone does NOT deploy. Use "make deploy" or "make" instead.
# See: feedback_restart_after_rebuild.md — 3 false "shipped" on 2026-04-05

.PHONY: deploy build restart verify test

deploy: build restart verify
	@echo "✓ Built, restarted, verified."

build:
	cargo build --release
	@codesign --force --sign "Chorus Local Signing" target/release/chorus-hooks 2>/dev/null && echo "✓ chorus-hooks signed" || echo "WARN: codesign failed (cert missing?)"
	@codesign --force --sign "Chorus Local Signing" target/release/chorus-hook-shim 2>/dev/null && echo "✓ chorus-hook-shim signed" || echo "WARN: codesign failed (cert missing?)"

restart:
	@launchctl kickstart -k gui/$$(id -u)/com.chorus.hooks 2>/dev/null || true
	@sleep 2

verify:
	@NEW_PID=$$(cat /tmp/chorus-hooks.pid 2>/dev/null); \
	if [ -z "$$NEW_PID" ]; then echo "FAIL: no PID file after restart"; exit 1; fi; \
	if ! ps -p $$NEW_PID > /dev/null 2>&1; then echo "FAIL: PID $$NEW_PID not running"; exit 1; fi; \
	echo "✓ chorus-hooks running (PID $$NEW_PID)"

test:
	cargo test
