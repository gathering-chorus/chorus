# Home Network Inventory

**Scanned:** 2026-02-19
**Network:** 192.168.86.0/24
**Router:** Google Nest WiFi (192.168.86.1)

---

## Computers (2)

| Device | IP | MAC | Role | Notes |
|--------|-----|-----|------|-------|
| Jeff's Mac Mini M1 (Library) | 192.168.86.36 | 14:98:77:34:bf:3a | Compute + Docker | M1, 16GB, 1.8TB SSD (76% used, 441GB free). 15 containers + 7 LaunchAgents. Wired Ethernet. |
| Jeff's Mac mini (Bedroom) | 192.168.86.242 | 5c:e9:1e:ea:72:11 | Media + storage | ~178TB external storage, 7TB Gathering mount. 3 LaunchAgents (images-api + volume-keepalive). WiFi. |

**Services on Library (192.168.86.36):** SMB file sharing, AirPlay receiver, 15 Docker containers, 7 LaunchAgents

---

## Network Infrastructure (3)

| Device | IP | MAC | Notes |
|--------|-----|-----|-------|
| Nest WiFi Router | 192.168.86.1 | b0:e4:d5:60:c5:bb | Gateway, DHCP, DNS |
| Nest WiFi Point | 192.168.86.20 | b0:e4:d5:8e:66:70 | Mesh extender |
| Nest WiFi Point | 192.168.86.49 | b0:e4:d5:8f:45:10 | Mesh extender |

---

## Apple TVs (3)

| Device | IP | MAC | Services |
|--------|-----|-----|----------|
| Living Room Apple TV | 192.168.86.22 | c8:d0:83:b4:f3:2c | AirPlay, RAOP |
| Bedroom AppleTV | 192.168.86.40 | a8:51:ab:09:63:ed | AirPlay, RAOP |
| Kitchen AppleTV | 192.168.86.43 | 1c:b3:c9:0d:78:c7 | AirPlay, RAOP |

---

## HomePods (5)

| Device | IP | MAC (from RAOP) | HomeKit Sensor |
|--------|-----|-----------------|----------------|
| Kitchen | 192.168.86.42 | DE:9E:8D:22:C7:60 | HomePodSensor (temp/humidity) |
| Kitchen (2) | 192.168.86.45 | 96:8C:EF:6C:1C:59 | HomePodSensor |
| Library HomePod | 192.168.86.38 | 16:92:C3:49:88:87 | HomePodSensor |
| Office HomePod | 192.168.86.239 | CE:9F:47:91:D5:AE | HomePodSensor |
| Attic HomePod | 192.168.86.41 | 12:AA:6E:5A:B2:02 | HomePodSensor |

All HomePods advertise AirPlay + RAOP (AirPlay audio) + HomeKit (HAP) sensor services.

---

## Entertainment (3)

| Device | IP | MAC | Notes |
|--------|-----|-----|-------|
| Pioneer SC-LX501 | 192.168.86.28 | (incomplete) | AV receiver. AirPlay + HTTP control. mDNS name: `pioneer-sc-lx501-4d829d` |
| Roku Ultra Living Room | 192.168.86.27 | (incomplete) | Streaming. AirPlay capable. |
| LG OLED48C2PUA | 192.168.86.250 | ac:5a:f0:65:13:ef | webOS TV. AirPlay capable. mDNS: `lgwebostv` |

---

## Smart Home / Speakers (2)

| Device | IP | MAC | Notes |
|--------|-----|-----|-------|
| Bedroom | 192.168.86.32 | d0:81:7a:e3:98:30 | AirPlay + RAOP. Likely HomePod Mini or speaker. |
| Bedroom (2) | 192.168.86.34 | d4:90:9c:c9:ab:36 | AirPlay + RAOP. Stereo pair with Bedroom. |

---

## Mobile Devices (2)

| Device | IP | MAC | Notes |
|--------|-----|-----|-------|
| iPhone | 192.168.86.31 | 4e:36:52:b5:0c:1e | Jeff's iPhone |
| iPhone | 192.168.86.33 | 8e:37:6c:7b:d8:91 | Second iPhone (Kathy?) |

---

## Other (2)

| Device | IP | MAC | Notes |
|--------|-----|-----|-------|
| HP OfficeJet Pro 9010 | 192.168.86.29 | 38:22:e2:9e:1a:38 | Printer. HTTP admin interface. |
| Apple Watch | 192.168.86.23 | 66:c0:de:fa:3d:ab | Jeff's watch |

---

## Docker Services on Library (15 containers)

| Container | Port | Status | Stack |
|-----------|------|--------|-------|
| jeff-bridwell-personal-site-app | 3000 | Healthy | Gathering app |
| jeff-bridwell-personal-site-fuseki | 3030 | Healthy | Gathering RDF/SPARQL |
| jeff-bridwell-personal-site-webvowl | 8089 | Healthy | Gathering ontology viz |
| wordpress-blog | 8081 | Healthy | Blog |
| wordpress-mysql | 3306 | Healthy | Blog DB |
| wordpress-mailhog | 1025, 8025 | Up (no health check) | Blog email testing |
| vikunja | 3456 | Healthy | Kanban board |
| prometheus | 9090 | Healthy | Metrics |
| grafana | 3100 | Healthy | Dashboards |
| loki | 3102 | Healthy | Log aggregation |
| promtail | 9080 | Healthy | Log shipping |
| alertmanager | 9093 | Healthy | Alert routing |
| node-exporter | 9100 | Healthy | Host metrics |
| blackbox-exporter | — | Healthy | Endpoint probing |
| mysqld-exporter | — | Healthy | MySQL metrics |

---

## Summary

| Category | Count |
|----------|-------|
| Computers | 2 |
| Network devices | 3 |
| Apple TVs | 3 |
| HomePods | 5 |
| Entertainment (AV/TV/Streaming) | 3 |
| Smart speakers (non-HomePod) | 2 |
| Mobile devices | 2 |
| Other (printer, watch) | 2 |
| Docker containers (Library) | 15 |
| LaunchAgents (Library) | 7 |
| LaunchAgents (Bedroom) | 3 |
| **Total network devices** | **22** |
| **Total managed services** | **47** |
