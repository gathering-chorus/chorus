# No app owns the door

**Position, 2026-08-08. Silas.** Recorded because Jeff has now said it twice —
2026-04-23: *"u keep putting this stuff in gathering and then pull into security
scope then act surprised that it doesnt work"*; and today: *"i really dont like
gathering being the core of security."*

## What is actually true today

Gathering is not the core of security. It holds exactly one thing it should not:
**the decision of where to send a visitor who is not recognized.** Two lines, in
two files:

- `jeff-bridwell-personal-site/src/app.ts` — the unauthenticated non-root branch
- `jeff-bridwell-personal-site/src/utils/auth-utils.ts` — the same URL, computed

Everything else already lives where it belongs. The guard owns identity, the
session cookie, the allow-set, the sign-in page, and the refusal. An app holding
a hardcoded sign-in URL is a routing rule that escaped into product code.

## Why it is not harmless

Today those two lines pointed at `/_auth/login` — the endpoint that bounces
straight to the identity provider — instead of `/_auth/`, the page we built. So
a stranger clicking into a Chorus page from the public site was walked to a
different domain asking for credentials, with no indication of what asked or
why. That is the shape of a phishing page whether or not it is one, and the
guard's own source carries a comment refusing to do it. The app in front did it
anyway.

Nobody on the allow-set ever saw it, because we all hold sessions. Jeff saw it
the first time he walked in cold.

Fixed today. The point is that it was *possible*: a security-relevant decision
sat in a place where nobody reviewing security would look, and where changing it
does not read as a security change.

## The shape it should have

**The guard fronts the domain and answers first. No app names a sign-in URL.**

An app receives requests that have already been decided about. It never
redirects to a door, never computes a return URL, never knows the provider
exists. If the door moves, no app changes. If a third app arrives, it inherits
the behaviour by being behind the guard rather than by copying two lines.

This is the same cut as `#3765` row 6, one level up. There, the rule was that
authentication is shared and authorization is local: an app decides who it
admits, and must not inherit that from another app. Here the rule is the
converse — an app must not decide where authentication *happens*, because that
is one decision for the whole deployment and every copy of it is a copy that can
drift.

## What this is not

Not a new project, and not a "shared security service." Jeff, 2026-06-12: *"just
dont want to do the new project now bc of impacts to gathering."* Still right.
This is a subtraction: two lines leave gathering and nothing replaces them
inside gathering.

It also does not survive contact with the one-host change unexamined. Jeff's
target is one host with paths — `lightlifeurbangardens.com`, `/chorus`,
`/gathering`, `/borg`.
That collapse makes this *easier*, because there is one front to sit behind
rather than three hostnames to keep consistent. It should be done as part of
that move, not before it.

## Standing risk, recorded

The gathering app also carries `/test/login`, a route that mints a session for
any WebID you name. It is gated on `PLAYWRIGHT_TEST_MODE`, and I verified that
against the running process rather than the source: the route answers 404
locally, and 302 through the tunnel because the guard refuses first. Two layers,
both holding.

It stays a standing risk rather than a closed item, because the thing that flips
it is an environment variable — a config change nobody would think of as a
security change. Same class as the two redirect lines above.
