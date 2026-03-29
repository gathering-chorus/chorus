# Electrical Outlet Expansion — Spec for Electrician

**Homeowner:** Jeff Bridwell
**Property:** Roslindale, MA
**Date:** 2026-02-25
**Panel:** Murray brand, Eaton BR120 breakers (20A/120V single-pole), open slots available

---

## Summary

Three rooms need additional outlets. Current state: too few outlets, daisy-chained power strips, sustained loads on single outlets. Goal: enough outlets so each gets one power strip max, no daisy chaining, and high-draw equipment on dedicated circuits.

**Total ask: 10-12 new outlets across 3-4 new 20A circuits.**

---

## Room 1: Bedroom (Server + Media Room)

### Current State
- **4 outlets**, daisy-chained power strips
- Sustained 24/7 load from computer and storage array

### Equipment

| Equipment | Watts | Duty |
|-----------|-------|------|
| Mac mini M2 Pro (computer) | 70 | 24/7 |
| 18 external USB hard drives in enclosures | 450 | 24/7 |
| Sony XBR-49X800E TV (49") | 85 | Intermittent |
| Apple TV 4K (2021) | 5 | Intermittent |
| 2x HomePod speakers | 20 | 24/7 standby |
| **Total** | **~630W** | |

### Concern
All 18 drives spin up simultaneously after a power event — inrush current spike. Computer and drives should not share a circuit with AV equipment.

### Recommendation
- **4 additional outlets** (8 total)
- **2 circuits minimum**: one for compute + drives (sustained 24/7), one for AV + general
- 20A circuits preferred (already existing breakers are 20A)

### Outlet Placement
- 4 outlets near desk/drive shelf area (compute circuit)
- 4 outlets near TV/entertainment area (AV circuit)

---

## Room 2: Living Room (AV / Stereo Stack)

### Current State
- **2 outlets**, power strips daisy-chained behind AV cabinet
- Heavy AV stack with 7-channel surround + subwoofer

### Equipment

| Equipment | Watts (peak/idle) | Duty |
|-----------|-------------------|------|
| Pioneer SC-LX501 AV receiver (7.2ch) | 260 / 80 | Intermittent |
| Sony XBR-55X800E TV (55") | 90 | Intermittent |
| Paradigm PDR10 v3 subwoofer (powered) | 150 / 15 | Intermittent |
| Sony BDP-S3500 Blu-ray player | 15 | Intermittent |
| Roku Ultra | 10 | Standby |
| Apple TV | 5 | Standby |
| Cisco CHS 435HDC (FIOS set-top) | 25 | 24/7 |
| Projekt turntable | 15 | Intermittent |
| **Total** | **~555W peak / ~255W idle** | |

*Note: 5 Paradigm speakers (center + 2 front + 2 rear) are passive — powered by the receiver, no outlets needed.*

### Recommendation
- **4 additional outlets** (6 total)
- **1-2 circuits**: one dedicated to AV stack, one for general room use (lamps, etc.)
- Outlets behind or near the AV cabinet — the current 2 are likely not positioned for the equipment

---

## Room 3: Basement (Garden / Grow Lights)

### Current State
- **1 outlet**, everything on one power strip
- Sustained load from grow lights on timers (12-16 hours/day)

### Equipment

| Equipment | Watts | Duty |
|-----------|-------|------|
| Barrina 4ft T8 LED grow light | 252 | 12-16 hr/day timer |
| 5-6 individual LED grow bulbs (~15W each) | 80 | 12-16 hr/day timer |
| **Total** | **~332W sustained** | |

### Concern
Grow lights are sustained-draw devices running half the day on timers. Should be on a dedicated circuit, not shared with general basement loads (tools, vacuum, etc.).

### Recommendation
- **2-4 additional outlets** (3-5 total)
- **1 dedicated 20A circuit** for grow lights
- GFCI protection if near water/irrigation (code may require this in basements)
- Outlet placement near grow area, high enough to avoid water

---

## Panel Notes

- **Panel brand:** Murray
- **Breakers:** Eaton Type BR120 (compatible with Murray panel)
- **Existing breakers:** 20A / 120V / single-pole
- **Open slots:** Available (exact count TBD — electrician to verify)
- **New circuits needed:** 3-4 total (20A each)

### Circuit Summary

| New Circuit | Room | Purpose | Amps |
|-------------|------|---------|------|
| 1 | Bedroom | Compute + drive array (24/7) | 20A |
| 2 | Bedroom | AV + general (if not on existing) | 20A |
| 3 | Living Room | AV stack | 20A |
| 4 | Basement | Grow lights (sustained timer load) | 20A GFCI |

*Electrician should verify whether existing bedroom circuits can absorb the AV load or if circuit 2 is needed. If existing circuits serve other rooms, a new dedicated circuit is safer.*

---

## Questions for Electrician

1. How many open slots remain in the panel after adding 3-4 breakers?
2. Are the existing bedroom outlets on one circuit or two? (Determines if we need 1 or 2 new bedroom circuits)
3. Basement GFCI requirement — is it code-required for the grow light area?
4. Can existing wiring be tapped for additional outlets, or does each room need a fresh home run to the panel?
5. Timeline and cost estimate for the full scope

---

*Prepared by Silas (infrastructure architect) — 2026-02-25*
