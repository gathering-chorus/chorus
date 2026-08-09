# ADR-056: Fitness functions per domain — Borg's paradigm

**Status**: Proposed — 2026-08-06
**Author**: Silas, from Jeff's direction ("for borg we want to use fitness functions as a paradigm for the underlying domains")
**Card**: #3765 builds the first instance (security)
**Relates to**: ADR-051 Addendum II (source exclusivity — a conformance rule of the same family), #3753 (load-aware measurement), #3742 (the week of instruments that lied)

## Context

Every domain in Chorus declares its vocabulary in OWL: the classes it owns, the shapes those
classes must satisfy, the properties they carry. That declaration is a specification. Nothing
checks it.

The consequence, measured repeatedly during 2026-08-04/06: the model and the running system drift
apart silently, and every diagnosis built on the assumption that they agree is a coin flip. Four
successive wrong diagnoses of one symptom (`/products` serving zero) each read a different tenancy
and each was internally consistent. The security domain declares a `Permission` class with exactly
the right shape — subject, action, resource, explicit-grant, owner — and holds zero instances, while
roughly thirty rules are enforced in Rust. The domain declares 29 `Gate` instances; 32 blocking
enforcement points run. Three modules document themselves as blocking and return allow.

None of that is a bug in the usual sense. Nothing crashed. The system simply stopped resembling its
own description, and no mechanism existed to notice.

Meanwhile the observability we do have reports confidently and is sometimes wrong: a probe that
cannot distinguish "the service is down" from "the machine was too busy to ask", a nightly that
cannot distinguish "the test failed" from "the test could not start", a security ledger whose six
green measures all cover the doors we built and none of the doors we didn't.

## Decision

**Borg's paradigm is one fitness function per domain, measuring conformance between what the domain
declares and what actually runs.**

A fitness function here is: an automated, objective, *graded* measure of an architectural
characteristic, run continuously, whose trend is recorded. Not a pass/fail test — a number and a
direction.

Four properties, all load-bearing:

1. **Model-derived.** The questions come from the domain's own OWL declaration, not from a
   hand-written checklist. A domain that declares seven classes has seven rows. Nobody decides what
   to measure; the model already did.

2. **Bidirectional.** Declared-but-absent fails exactly as loudly as present-but-undeclared. The
   second direction is the one always forgotten, and it is where drift actually accumulates —
   nobody forgets to build the thing they declared; everybody forgets to declare the thing they
   built.

3. **Graded and trended.** The output is a score plus per-row detail, emitted each run so the
   direction is queryable. A pass/fail test cannot see a decline from 4-of-7 to 3-of-7; that
   decline is precisely what this exists to catch, because nothing broke — something drifted.

4. **Honest about the unmeasurable.** A fitness function that cannot reach the store reports
   UNMEASURABLE, never green and never red. The 2026-08-05 lesson: instruments that report
   confidently under conditions they cannot measure in are worse than no instruments, because we
   act on them.

**Ownership follows the domain.** The role that owns a domain owns its fitness function. Silas owns
security's; the shape is shared, the questions are the domain's.

## What this is not

- **Not a coverage metric.** Coverage asks whether a test touched a line. Conformance asks whether
  the running system matches its declaration. A line can be covered by a test that asserts nothing;
  a conformance row cannot be satisfied without changing reality or changing the declaration.
- **Not proof of correctness.** A green fitness function proves the system and the model agree. It
  says nothing about whether the model is *right*. Declare a bad permission and the function will
  cheerfully confirm we are following it. That judgment stays human. What it removes is being wrong
  *by accident*, which is how every failure of the 08-04/06 week happened.
- **Not a dashboard.** The number is emitted and trended; a surface may render it. The measurement
  is the artifact, not the picture.

## Consequences

- Each domain's chunk of work gains a finish line: the chunk is done when its fitness function is
  green, not when someone runs out of card ideas. Cards exist because a row is red, and cards that
  map to no row are either outside the goal or the goal is incomplete — either way the mapping says
  so. (Applied to security on 2026-08-06: 15 open cards, 12 mapped, 3 mapped to nothing, 2 rows
  entirely unstaffed.)
- The nightly gains a second kind of question. Today it asks "do the tests pass." It will also ask
  "does each domain still resemble itself."
- A domain with no declaration has no fitness function, which is itself a finding. The absence is
  visible instead of assumed.
- Cost is real and bounded: one function per domain, sharing one implementation shape. The security
  instance (#3765) is the reference; the second domain should be cheap or the shape is wrong.

## First instance

Security, seven rows (Gate, Permission, Principal, Credential, APISurface, AuthBoundary,
SecurityProbe), measured 2026-08-06 at **1 of 7 conforming**. The measurement was taken before the
mechanism was built, so the starting score is honest rather than flattering — and the number is
expected to rise slowly, because most of the work is declaring what we already enforce.
