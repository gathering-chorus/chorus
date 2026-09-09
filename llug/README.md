# Light Life Urban Gardens

A garden design and maintenance practice in Roslindale, Massachusetts. This repo holds
the model the software is generated from, the documents that go to clients, and the
survey work behind both.

Everything here started as a real engagement: three households walked in September
2026, one job booked, hand-drawn maps for each.

## What is here

```
docs/     model.html        the value stream (SADIMET) and seven domains as classes,
                            attributes and typed edges — what the app generates from
          build-spec.html   how it gets built: three chunks, one photo end to end,
                            the six field screens, and the engineering picks
          app-mock.html     the field app, six screens: arrive, capture, identify,
                            work, close, and the evening write-up

clients/  pinedale-proposal.html      a client proposal, brand sheet, one page
          pinedale-field-plates.html  ten photos of one garden, read one per page
          albano-survey.html          a site survey with the hand map embedded

survey/   the findings, with their evidence, so they can be re-checked
```

## Not done yet

The plant identification probe. One photo, one real call to Pl@ntNet, and an honest
verdict on whether the candidates are good enough to build on. It needs an API key,
which is Jeff's to get. Until then this repo says nothing about identification, and
the two findings in `survey/` cover name MATCHING only — what happens after a name
exists, not where the name comes from.

## Status

**This is a survey, not a build.** Chorus card #4119. Nothing generates yet; the point
of this stage is to find out whether the model holds up outside Chorus and whether the
pieces we would depend on actually work. Findings are in `survey/`, each one carrying
the data it was derived from rather than a claim to trust.

## The stream

Permaculture's own design cycle, because this trade already has one:

```
Survey · Analysis · Decisions · Implement · Maintain · Evaluate · Tweak
                                                  ↺ back to Analysis
```

A property is surveyed once; every season after re-reads it. That difference is what
turns a customer into a client.
