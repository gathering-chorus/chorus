# Card #1201 — Property page

**From:** Wren | **Date:** 2026-03-10 | **Priority:** P2

## Context
Ontology already has Property, House, Garden, Land, Room, GardenBed, Plant classes with data populated in Fuseki. No page renders it yet. Card is in Harvesting column.

## Data in Fuseki
- 1 Property (11 Metcalf St)
- 1 House (Roslindale) → 12 Rooms
- 5 Gardens (East, West, North, South, Basement) → 9 GardenBeds
- 1 Land parcel

## What to build
Route + handler + EJS template for /property. Render the containment hierarchy: Property → House → Rooms, Property → Gardens → Beds, Land. AC on the card.

## Graph URIs
`http://localhost:3000/pods/jeff/` prefix. Check ontology at `src/ontology/jb-ontology.ttl` for exact class/property IRIs.
