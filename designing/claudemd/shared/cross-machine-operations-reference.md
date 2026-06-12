## Cross-Machine Reference

### SSH Examples (always safe — read-only)
```bash
ssh jeffbridwell@Jeffs-Mac-mini.local (192.168.86.242 reserved) "pgrep -la node"
ssh jeffbridwell@Jeffs-Mac-mini.local (192.168.86.242 reserved) "curl -s http://localhost:8082/health"
ssh jeffbridwell@Jeffs-Mac-mini.local (192.168.86.242 reserved) "tail -20 /tmp/images-api-server.log"
ssh jeffbridwell@Jeffs-Mac-mini.local (192.168.86.242 reserved) "df -h"
```

### Service Registry (Library LaunchAgents)
- `com.chorus.api` — Chorus context index HTTP API (KeepAlive)
- `com.chorus.alert-notifier` — Alert notifications (KeepAlive)
- `com.chorus.session-watcher` — Ambient index daemon (KeepAlive)
- `com.chorus.defect-poller` — Error polling (KeepAlive)
- `com.chorus.fuseki-perf` — RDF store monitoring (KeepAlive)
- `com.chorus.fuseki-compact` — TDB2 weekly compact, Saturday 1am (StartCalendarInterval)
- `com.chorus.ops-agent` — Operational health (KeepAlive)

### Service Registry (Bedroom LaunchAgents)
- `com.gathering.images-api-server` — Gallery UI, port 3001 (KeepAlive)
- `com.gathering.images-api-video` — Media serving, port 8082 (KeepAlive)
- `com.gathering.volume-keepalive` — USB enclosure idle prevention, every 4min (run-once, repeating)
