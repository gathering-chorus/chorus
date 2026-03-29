# Concept Enrichment Ontology Proposal — #1121

**From:** Wren | **Date:** 2026-03-07 | **Status:** Draft for Jeff review

## What This Adds

The existing ontology models *what* Jeff has (tracks, photos, stories, notes) and *how he relates to it* (authored, collected, captured, told). This enrichment adds *how it feels* — emotional, somatic, psychological, and philosophical dimensions.

## Design Principle

Reuse established vocabularies where they exist. Only mint jb: terms for concepts unique to Jeff's system. Keep it small — 15 classes/properties, not 150.

---

## Layer 1: Emotional Annotation (from Onyx + MFOEM)

Onyx is Linked Data native. MFOEM is the scientific backbone. We use Onyx for annotation structure and MFOEM for emotion taxonomy.

### New prefixes
```turtle
@prefix onyx: <http://www.gsi.upm.es/ontologies/onyx/ns#> .
@prefix mfoem: <http://purl.obolibrary.org/obo/MFOEM_> .
```

### Classes
| Class | Source | Purpose |
|-------|--------|---------|
| `onyx:EmotionSet` | Onyx | Container for emotions detected on a resource |
| `onyx:Emotion` | Onyx | A single emotion annotation |
| `jb:SentimentScore` | New | Numeric sentiment value (positive/negative/neutral) attached to any resource |

### Properties
| Property | Domain | Range | Purpose |
|----------|--------|-------|---------|
| `onyx:hasEmotionSet` | any resource | `onyx:EmotionSet` | Links a story, blog post, capture to its emotional annotation |
| `onyx:hasEmotion` | `onyx:EmotionSet` | `onyx:Emotion` | Individual emotion within the set |
| `onyx:hasEmotionCategory` | `onyx:Emotion` | URI | Points to MFOEM emotion class (joy, grief, anxiety, etc.) |
| `onyx:hasEmotionIntensity` | `onyx:Emotion` | `xsd:float` | 0.0–1.0 intensity |
| `jb:sentimentValue` | `jb:SentimentScore` | `xsd:float` | -1.0 (negative) to +1.0 (positive) |
| `jb:sentimentSource` | `jb:SentimentScore` | `xsd:string` | "ollama/mistral-7b", "manual", etc. |
| `jb:hasSentiment` | any resource | `jb:SentimentScore` | Links resource to its sentiment annotation |

### Example
```turtle
<http://localhost:3000/pods/jeff/stories/christmas-stlouis-2014>
    onyx:hasEmotionSet [
        a onyx:EmotionSet ;
        onyx:hasEmotion [
            a onyx:Emotion ;
            onyx:hasEmotionCategory mfoem:0000024 ;  # grief
            onyx:hasEmotionIntensity 0.6
        ] , [
            a onyx:Emotion ;
            onyx:hasEmotionCategory mfoem:0000028 ;  # love
            onyx:hasEmotionIntensity 0.8
        ]
    ] ;
    jb:hasSentiment [
        a jb:SentimentScore ;
        jb:sentimentValue 0.3 ;
        jb:sentimentSource "ollama/mistral-7b"
    ] .
```

---

## Layer 2: Somatic / Body State (new jb: terms, inspired by Damasio)

No existing OWL ontology covers somatic markers. Small bespoke vocabulary.

### Classes
| Class | Purpose |
|-------|---------|
| `jb:SomaticMarker` | A body-state signal associated with a memory or experience |
| `jb:BodyState` | Named body state: tense, relaxed, energized, depleted, agitated, calm |

### Properties
| Property | Domain | Range | Purpose |
|----------|--------|-------|---------|
| `jb:hasSomaticMarker` | any resource | `jb:SomaticMarker` | Links resource to a body-state annotation |
| `jb:bodyState` | `jb:SomaticMarker` | `jb:BodyState` | The named state |
| `jb:somaticIntensity` | `jb:SomaticMarker` | `xsd:float` | 0.0–1.0 |

### Named individuals (extensible)
```turtle
jb:Tense a jb:BodyState ; rdfs:label "tense" .
jb:Relaxed a jb:BodyState ; rdfs:label "relaxed" .
jb:Energized a jb:BodyState ; rdfs:label "energized" .
jb:Depleted a jb:BodyState ; rdfs:label "depleted" .
jb:Calm a jb:BodyState ; rdfs:label "calm" .
jb:Agitated a jb:BodyState ; rdfs:label "agitated" .
```

---

## Layer 3: Buddhist Psychological Vocabulary (new jb: terms, inspired by Abhidharma)

The five aggregates (skandhas) map to how Jeff already thinks about memory — not just facts but sensation, perception, mental formation, consciousness.

### Classes
| Class | Sanskrit | Purpose |
|-------|----------|---------|
| `jb:Vedana` | vedanā | Feeling tone — pleasant, unpleasant, neutral. The raw valence of an experience before interpretation. |
| `jb:Samjna` | saṃjñā | Perception/recognition — how the mind labels and categorizes. |
| `jb:Samskara` | saṃskāra | Mental formation — habitual patterns, volition, the "why" behind behavior. Connects to perseveration/grit. |

### Properties
| Property | Domain | Range | Purpose |
|----------|--------|-------|---------|
| `jb:hasVedana` | any resource | `jb:Vedana` | Feeling tone annotation |
| `jb:vedanaTone` | `jb:Vedana` | `xsd:string` | "pleasant", "unpleasant", "neutral" |
| `jb:hasSamskara` | any resource | `jb:Samskara` | Pattern/formation annotation |
| `jb:samskaraLabel` | `jb:Samskara` | `xsd:string` | Named pattern: "persistence", "care", "vigilance" |

---

## Layer 4: Philosophical Concepts (new jb: terms, from Jeff's reading)

### Classes
| Class | Source | Purpose |
|-------|--------|---------|
| `jb:Assemblage` | Deleuze & Guattari | A configuration of heterogeneous elements that work together — the team, the system, a pod |
| `jb:Bricolage` | Lévi-Strauss | Something built from what's at hand — improvised, functional, shaped by use |

### Properties
| Property | Domain | Range | Purpose |
|----------|--------|-------|---------|
| `jb:hasAssemblage` | any resource | `jb:Assemblage` | Tags something as part of an assemblage |
| `jb:isBricolage` | any resource | `xsd:boolean` | Marks something as improvised/assembled from parts |

---

## What This Enables

1. **Sentiment pipeline** (#1121) — Ollama processes graph text → writes Onyx + SentimentScore triples
2. **Link inference** (#1123) — emotion similarity becomes an edge-discovery signal (two stories with grief+love are related even if they share no keywords)
3. **Vedana as a first-class filter** — "show me everything that felt unpleasant" across all domains
4. **Somatic markers on the andon** — Jeff's current body state connects to his historical pattern
5. **Jeff's philosophical frame** — assemblage and bricolage as navigable concepts in the graph

## Total New Terms

- **3 classes** from external vocabularies (Onyx)
- **12 new jb: classes** (SentimentScore, SomaticMarker, BodyState, Vedana, Samjna, Samskara, Assemblage, Bricolage + 6 BodyState individuals)
- **14 new properties**

Small enough to implement in one pass. Rich enough to change how the graph feels.
