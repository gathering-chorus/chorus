# #3499 one-run pipeline proof (#3510)

This file is the trivial change that proves the collapsed werk pipeline lands a real
card end-to-end in ONE run: go-at-invoke → werk-demo blocks-on-proving (peer gathers
reply, same round) → proven → merge → sync → deploy → accept. No second invocation,
no witness wall.

Landed: 2026-06-19, via the live one-run werk.yml + pure verbs + blocking demo (#3499).
