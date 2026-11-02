#!/bin/sh
# Syntax-checks every UI module with node (ES modules need an .mjs copy for --check).
set -e
ui="$(dirname "$0")/../app/ui/js"
tmp="$(mktemp -d)"
n=0
for f in "$ui"/*.js "$ui"/pages/*.js "$ui"/overlay/*.js; do
  cp "$f" "$tmp/$(basename "$f" .js)-$n.mjs"
  node --check "$tmp/$(basename "$f" .js)-$n.mjs"
  n=$((n + 1))
done
rm -rf "$tmp"
echo "ui modules ok: $n"
