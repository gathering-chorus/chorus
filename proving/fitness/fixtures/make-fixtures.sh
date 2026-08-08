#!/usr/bin/env bash
# Fixtures for row 1, generated rather than committed as blobs so it is visible
# what each one is FOR.
#
# On #3725 the first fixture written to prove a check still caught a violation
# was itself bogus — it "failed" for a reason unrelated to the rule. So each
# fixture here isolates ONE assertion: it satisfies every other condition, so a
# red can only be attributed to the thing the fixture is named after.
set -euo pipefail
cd "$(dirname "$0")"

nav='<nav><a href="/about">About</a> <a href="/blog">Blog</a> <a href="/chorus">Chorus</a></nav>'
pad=$(printf 'x%.0s' $(seq 1 5000))

# ONLY the credential-form assertion may fire: full nav, well over the size floor.
{
  echo "<!doctype html><html><head><title>Sign in</title></head><body>"
  echo "$nav"
  echo '<form method="post" action="/login">'
  echo '<input type="text" name="username">'
  echo '<input type="password" name="password">'
  echo '<button>Sign in</button></form>'
  echo "<!-- $pad -->"
  echo "</body></html>"
} > login-wall-otherwise-perfect.html

# ONLY the navigation assertion may fire: big, no credential form, but the site's
# own pages are gone — a placeholder or a wrong vhost.
{
  echo "<!doctype html><html><head><title>It works!</title></head><body>"
  echo "<h1>It works!</h1><p>Default page.</p>"
  echo "<!-- $pad -->"
  echo "</body></html>"
} > wrong-site-otherwise-perfect.html

# The real shape we lived: a login wall in full — small, no nav, password field.
{
  echo "<!doctype html><html><head><title>Sign in</title></head><body>"
  echo '<form method="post"><input type="password" name="password"><button>Sign in</button></form>'
  echo "</body></html>"
} > root-login-wall.html

echo "fixtures written:"
wc -c login-wall-otherwise-perfect.html wrong-site-otherwise-perfect.html root-login-wall.html
