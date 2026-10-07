# #4358 — fills every gap in principles-4186-violations.ttl so the bats can show
# the fixture conforms once fixed: a red on the raw fixture is the rule firing.
import sys

t = open(sys.argv[1]).read()
t = t.replace(
    'rdfs:comment "Use protracted and thoughtful observation." ;\n    chorus:order 1 .',
    'rdfs:comment "Use protracted and thoughtful observation." ;\n'
    '    chorus:source "Hemenway, T. Gaia\'s Garden, 2nd ed., p. 6." ;\n'
    '    chorus:techReading "Watch a role work before directing it, read the board first." ;\n'
    '    chorus:jeffReading "Jeff reads the board and listens before committing to a move." ;\n'
    '    chorus:order 1 .',
)
t = t.replace('adjacency is the design." .', 'adjacency is the design." ; chorus:order 2 .')
t = t.replace('chorus:xp-fixture-humanity', 'chorus:hemenway-fixture-humanity')
t = t.replace('2nd ed. (Chelsea Green, 2009), p. 6."', '2nd ed., p. 6."')
t = t.replace('chorus:order 15 .', 'chorus:order 14 .')
open(sys.argv[2], 'w').write(t)
